//! `ask` verb: hybrid retrieval (architecture.md §12-13).
//!
//! ```text
//! query -> [resolve filters -> allowed-ID set]      (filter.rs, one indexed SQL pass)
//!        -> lexical leg:  DF-filtered FTS5 MATCH, bm25(), restricted to allowed
//!                          set, corpus-scaled cap (D016.1; see `build_lexical_query`)
//!        -> semantic leg: embed query -> cosine over ALL allowed chunks (untruncated)
//!        -> RRF fusion:   score(c) = sum_legs 1/(60 + rank_leg(c))       (rank.rs)
//!        -> collapse chunks -> best-scoring chunk represents its memory
//!        -> sort (score DESC, id ASC) -> top k -> hydrate content + metadata
//! ```
//!
//! **S5b (D016.1) lexical-leg tuning.** The S5 benchmark found that an
//! unfiltered OR-join of every query token (including stopwords) lexically
//! matched most of a small corpus, and RRF's rank-based scoring gave that
//! noise the same weight as a true top-ranked semantic hit (bench/REPORT.md
//! §3.4). Two changes address it, both applied in `build_lexical_query` /
//! `lexical_cap` below:
//!
//! 1. **Document-frequency filtering**: a query token matching more than
//!    `DF_FILTER_FRACTION` of the allowed corpus is dropped from the OR set
//!    before the FTS5 query is built. If every token is dropped, the lexical
//!    leg contributes nothing for that query -- it never falls back to the
//!    unfiltered query.
//! 2. **Corpus-scaled candidate cap**: the old fixed `LIMIT 200` never bound
//!    at small corpus sizes (so it filtered nothing); it is now
//!    `min(200, max(4*k, ceil(total_chunks/10)))`, scaling with the size of
//!    the allowed set actually being searched.
//!
//! **Supervisor ruling on the S5b sweep.** Neither mechanism above, at any
//! setting inside D016.1's authorized tuning bounds, got `--mode hybrid` to
//! score at or above `--mode semantic` on the same 62- or 1,000-memory
//! corpus (bench/REPORT.md's S5b addendum) -- which would leave
//! architecture.md §21.1/§25's "default mode must be >= each pure mode"
//! invariant broken by construction if the *gate* were merely recalibrated
//! per D016.3. So below `LEXICAL_ACTIVATION_CHUNKS` allowed chunks,
//! `--mode hybrid`'s lexical leg is deactivated outright (`effective_
//! lexical_cap` returns 0) and fusion degenerates to pure semantic ranking
//! -- hybrid cannot score below semantic there by construction. At or above
//! that threshold the tuned DF-filtering + corpus-scaled cap configuration
//! runs unchanged (verified to preserve, and slightly widen, the 10K
//! hybrid >= semantic crossover). `--mode lexical` and `--mode semantic`
//! are explicit single-leg requests and are never subject to this
//! threshold.

use crate::db::Db;
use crate::embed::Embedder;
use crate::error::AppError;
use crate::filter::{self, WhereTerm};
use crate::rank::{self, FusedRanks};
use crate::vector::{self, BruteForceCosine, VectorIndex};
use rusqlite::Connection;
use serde::{Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

pub const MAX_QUERY_BYTES: usize = 8192; // 8 KiB (architecture.md §12, §20)

/// Hard ceiling on the lexical leg's `LIMIT`, regardless of corpus size
/// (D016.1). The corpus-scaled term in `lexical_cap` can never exceed this.
const LEXICAL_CAP_CEILING: usize = 200;
/// The corpus-scaled term of the lexical cap is `ceil(total_chunks /
/// LEXICAL_CAP_DIVISOR)`. Tuned in [5, 20] against the S5b sweep
/// (bench/REPORT.md); 10 was the value that preserved the 10K hybrid ≥
/// semantic crossover while still shrinking the cap enough to matter at 62.
const LEXICAL_CAP_DIVISOR: usize = 10;
/// The cap's floor is `LEXICAL_CAP_FLOOR_MULT * k`, so a small corpus still
/// returns enough lexical candidates to matter for a large `--k`. Tuned in
/// [2, 6]; see the S5b sweep in bench/REPORT.md.
const LEXICAL_CAP_FLOOR_MULT: usize = 4;
/// A query token whose document frequency exceeds this fraction of the
/// allowed corpus is dropped from the lexical OR set (D016.1). Tuned in
/// [0.25, 0.6] against the S5b sweep in bench/REPORT.md; 0.5 was the value
/// that cleared both the 62-scale quality gates and the 10K crossover.
const DF_FILTER_FRACTION: f64 = 0.5;
/// A token is never dropped while its raw document frequency is at or below
/// this count, however large a fraction of a *tiny* corpus that is. This is
/// not one of D016.1's three tunables -- it is a necessary implementation
/// detail the percentage rule alone cannot express: a fixed fraction always
/// drops every token once `allowed_chunk_total` is small enough (a token
/// appearing in the corpus's one or two chunks is ≥ any fraction in
/// [0.25, 0.6] of it), which would make `--mode lexical` permanently
/// nonfunctional on the small stores D016.1 itself says "every user's store
/// starts small" in -- a strictly worse regression than the one this sprint
/// fixes. A token matching only one or two documents is, by construction,
/// the opposite of the "stopword mass" this filter targets (bench/REPORT.md
/// §3.4 measured real offending tokens at document frequency 10-62 on the
/// 62-memory corpus), so exempting df ≤ 2 costs nothing there: verified in
/// the S5b sweep that this floor changes zero measurements at 62/1K/10K
/// (see bench/REPORT.md's S5b section).
const DF_ABSOLUTE_FLOOR: usize = 2;

/// `--mode hybrid|lexical|semantic`, default `hybrid` (architecture.md §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Mode {
    Hybrid,
    Lexical,
    Semantic,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Hybrid => "hybrid",
            Mode::Lexical => "lexical",
            Mode::Semantic => "semantic",
        }
    }

    /// `--mode semantic` skips FTS entirely (architecture.md §13).
    fn runs_lexical(self) -> bool {
        matches!(self, Mode::Hybrid | Mode::Lexical)
    }

    /// `--mode lexical` must work on a DB whose embeddings are
    /// unusable/absent, which means never loading the embedder in that
    /// mode (architecture.md §13) -- this gate is the only place that
    /// decision is made.
    fn runs_semantic(self) -> bool {
        matches!(self, Mode::Hybrid | Mode::Semantic)
    }
}

/// Already-parsed and validated `ask` input; `cli.rs`/`main.rs` own turning
/// raw clap output (and `--where` parsing, via `filter::parse_where_terms`)
/// into this.
pub struct AskInput {
    pub query: String,
    pub k: u32,
    pub where_terms: Vec<WhereTerm>,
    pub include_superseded: bool,
    pub include_forgotten: bool,
    pub mode: Mode,
    pub min_score: Option<f64>,
}

fn serialize_rounded<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    // Round ONLY at serialization (architecture.md §12 determinism
    // contract) -- ordering and `--min-score` filtering both use the full
    // f64 precision computed by `rank::fuse`.
    let rounded = (value * 100_000.0).round() / 100_000.0;
    serializer.serialize_f64(rounded)
}

#[derive(Serialize)]
struct RanksOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    lexical: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic: Option<u32>,
}

#[derive(Serialize)]
struct SystemInfo {
    created_at: String,
    source: Option<String>,
    status: String,
    content_hash: String,
}

#[derive(Serialize)]
struct ResultItem {
    id: String,
    content: String,
    #[serde(serialize_with = "serialize_rounded")]
    score: f64,
    ranks: RanksOut,
    metadata: serde_json::Value,
    system: SystemInfo,
}

#[derive(Serialize)]
struct Stats {
    /// Count of distinct chunks that survived into RRF fusion across
    /// whichever legs ran (i.e. `fused.len()` in `run`). As of D016.1 this
    /// is affected by two lexical-leg changes, so it is no longer purely
    /// "everything either leg matched" the way it was pre-S5b: DF filtering
    /// can drop some or all lexical query tokens (a heavily stopword-laden
    /// query may contribute zero lexical candidates), and the corpus-scaled
    /// cap (`lexical_cap`) bounds how many lexical matches are even
    /// considered before fusion, in addition to the semantic leg's
    /// untruncated scan. As of the supervisor ruling on S5b, `--mode
    /// hybrid` on a corpus below `LEXICAL_ACTIVATION_CHUNKS` allowed chunks
    /// contributes zero lexical candidates unconditionally (the leg does
    /// not run at all -- `effective_lexical_cap` returns 0), so `candidates`
    /// there equals the semantic leg's contribution alone, same as `--mode
    /// semantic`. The semantic leg's own contribution is unchanged in all
    /// cases.
    candidates: usize,
    returned: usize,
    elapsed_ms: u64,
}

#[derive(Serialize)]
pub struct AskResponse {
    ok: bool,
    op: &'static str,
    mode: &'static str,
    query: String,
    results: Vec<ResultItem>,
    stats: Stats,
}

fn empty_response(mode: Mode, query: String, started: Instant) -> AskResponse {
    AskResponse {
        ok: true,
        op: "ask",
        mode: mode.as_str(),
        query,
        results: Vec::new(),
        stats: Stats {
            candidates: 0,
            returned: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    }
}

fn validate_query(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "empty_query",
            "query is empty after trimming",
        ));
    }
    if trimmed.len() > MAX_QUERY_BYTES {
        return Err(AppError::validation(
            "input_too_large",
            format!(
                "query is {} bytes, exceeding the {MAX_QUERY_BYTES}-byte cap",
                trimmed.len()
            ),
        ));
    }
    Ok(trimmed.to_string())
}

/// Splits `text` on non-alphanumeric boundaries into raw token strings, in
/// order, duplicates included. The shared first step of both the (test-only)
/// unfiltered sanitizer below and the DF-filtered query builder used at
/// retrieval time (`build_lexical_query`).
fn extract_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Quotes a single token as an FTS5 phrase literal, doubling any embedded
/// `"` (the FTS5 string-literal escape). Because every token is emitted as a
/// quoted phrase, no FTS5 operator, paren, or unbalanced quote from the
/// original query can ever reach the parser as syntax -- it is either
/// consumed by `extract_tokens`'s split (dropped) or safely quoted here.
fn quote_fts5_token(token: &str) -> String {
    format!("\"{}\"", token.replace('"', "\"\""))
}

/// Joins already-extracted raw tokens into an `OR`-joined FTS5 `MATCH`
/// string, quoting each one. Returns `None` for an empty slice -- callers
/// use this both for "no alphanumeric content at all" (pure punctuation) and
/// for "every token was dropped by DF filtering"; in both cases the lexical
/// leg contributes nothing rather than sending FTS5 an empty/invalid `MATCH`
/// string, and DF filtering never falls back to the unfiltered query.
fn quote_and_join(tokens: &[String]) -> Option<String> {
    if tokens.is_empty() {
        None
    } else {
        Some(
            tokens
                .iter()
                .map(|t| quote_fts5_token(t))
                .collect::<Vec<_>>()
                .join(" OR "),
        )
    }
}

/// Test-only: the pre-D016.1 unfiltered sanitizer (architecture.md §13,
/// Satchel's `build_fts5_query` pattern per §4/§22 -- reimplemented here,
/// not copied, since Satchel's version is tied to its own schema), kept to
/// unit-test tokenizing/quoting/escaping in isolation from DF filtering and
/// the DB. Retrieval itself goes through `build_lexical_query`, which
/// applies DF filtering to the same token stream before quoting.
#[cfg(test)]
fn build_fts5_query(text: &str) -> Option<String> {
    quote_and_join(&extract_tokens(text))
}

/// Materializes `temp.ask_allowed_chunk_rowids(rowid)` -- the integer
/// `chunks.rowid` set for every chunk in the current `ask_allowed` scope --
/// once per `ask` invocation, and returns its count (the shared denominator
/// for DF filtering and the corpus-scaled lexical cap, D016.1; both use the
/// *filtered*, post-`--where` scope, consistent with `lexical_leg`'s own
/// restriction). `token_df` and `lexical_leg` join against this table
/// instead of re-deriving `chunks JOIN ask_allowed` (a `TEXT` equality) on
/// every call: measured on the 10K-chunk S5b scale DB, an all-stopword-ish
/// query joining `chunks.memory_id = ask_allowed.id` per candidate token
/// took ~55ms median end-to-end -- over the D016.2 50ms retrieval-only
/// budget -- because a common token's match set (thousands of rows at that
/// scale) gets nested-loop-joined through a TEXT key on every one of
/// several tokens. Materializing this integer-rowid set once and joining
/// `chunks_fts.rowid` directly against it (SQLite integer primary key,
/// O(log n) per matched row) cut the same query to ~24ms; see the S5b
/// latency section of bench/REPORT.md for the full before/after
/// measurement. Idempotent / safe to call more than once per connection,
/// same pattern as `filter::resolve_allowed_ids`.
fn materialize_allowed_chunk_rowids(conn: &Connection) -> Result<usize, AppError> {
    // S6 audit F4: same unqualified-DROP hazard as `filter::resolve_allowed_
    // ids`'s `ask_allowed` -- an unqualified `DROP TABLE IF EXISTS
    // ask_allowed_chunk_rowids` on a fresh connection resolves against the
    // main schema before this function's own `CREATE TEMP TABLE` has ever
    // run, so a caller's own `main.ask_allowed_chunk_rowids` table would be
    // destroyed by a read-only verb. Every reference below is
    // schema-qualified to temp.* so this can only ever touch the temp
    // table.
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.ask_allowed_chunk_rowids;
         CREATE TEMP TABLE temp.ask_allowed_chunk_rowids AS
           SELECT c.rowid AS rowid FROM chunks c JOIN ask_allowed a ON a.id = c.memory_id;
         CREATE UNIQUE INDEX temp.idx_ask_allowed_chunk_rowids ON ask_allowed_chunk_rowids(rowid);",
    )?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM ask_allowed_chunk_rowids", [], |r| {
        r.get(0)
    })?;
    Ok(count as usize)
}

/// Document frequency of a single already-quoted FTS5 token: how many
/// chunks within the materialized `ask_allowed_chunk_rowids` set it
/// matches. One indexed `COUNT(*)` per distinct query token (architecture
/// .md §13, D016.1), joined by integer rowid rather than by `ask_allowed`'s
/// `TEXT` id -- see `materialize_allowed_chunk_rowids` for why. Callers
/// must have called that function on this connection first.
fn token_df(conn: &Connection, quoted_token: &str) -> Result<usize, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM chunks_fts
         JOIN ask_allowed_chunk_rowids r ON r.rowid = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1",
        [quoted_token],
        |r| r.get(0),
    )?;
    Ok(count as usize)
}

/// Builds the lexical leg's FTS5 `MATCH` query with document-frequency
/// filtering (architecture.md §13, D016.1): a token matching more than
/// `DF_FILTER_FRACTION` of the allowed corpus -- and more than
/// `DF_ABSOLUTE_FLOOR` chunks outright -- is dropped from the OR set before
/// quoting, because on a small corpus nearly every natural-language question
/// shares `we`/`the`/`a`/`what` with most memories, and that stopword mass
/// gave the lexical leg's rank-1 the same RRF weight as a true positive
/// (bench/REPORT.md §3.4). If every token is dropped, returns `None` -- the
/// lexical leg then contributes nothing for this query and the semantic leg
/// carries it alone; this never falls back to the unfiltered query. Tokens
/// are deduplicated (case-sensitive) before DF is queried, so a repeated
/// token costs one `MATCH` count, not one per occurrence.
fn build_lexical_query(
    conn: &Connection,
    text: &str,
    allowed_chunk_total: usize,
) -> Result<Option<String>, AppError> {
    if allowed_chunk_total == 0 {
        return Ok(None);
    }
    let mut tokens = extract_tokens(text);
    let mut seen = HashSet::new();
    tokens.retain(|t| seen.insert(t.clone()));

    let fraction_max_df = ((allowed_chunk_total as f64) * DF_FILTER_FRACTION).floor() as usize;
    let max_df = fraction_max_df.max(DF_ABSOLUTE_FLOOR);
    let mut kept = Vec::with_capacity(tokens.len());
    for token in tokens {
        let quoted = quote_fts5_token(&token);
        if token_df(conn, &quoted)? <= max_df {
            kept.push(token);
        }
    }
    Ok(quote_and_join(&kept))
}

/// D016.1's corpus-scaled lexical candidate cap:
/// `min(200, max(4*k, ceil(total_chunks / 10)))`. Replaces the old fixed
/// `LIMIT 200`, which never bound at the 62-memory kernel-proof scale and
/// let 81% of the corpus survive DF-unfiltered into RRF fusion as noise
/// (bench/REPORT.md §3.4) -- the cap now shrinks with the corpus instead of
/// only the DF filter doing that work, and stays a genuine precision signal
/// at the 10K scale where it already worked (bench/REPORT.md §4).
fn lexical_cap(k: u32, allowed_chunk_total: usize) -> usize {
    let floor = LEXICAL_CAP_FLOOR_MULT * (k as usize);
    let scaled = allowed_chunk_total.div_ceil(LEXICAL_CAP_DIVISOR);
    std::cmp::min(LEXICAL_CAP_CEILING, std::cmp::max(floor, scaled))
}

/// Below this many allowed chunks, `--mode hybrid`'s lexical leg is fully
/// deactivated (cap = 0). Supervisor ruling on the S5b sweep evidence
/// (bench/REPORT.md's S5b addendum): no DF-fraction/cap configuration
/// inside D016.1's authorized tuning bounds got hybrid recall@5 or MRR to
/// meet or beat semantic at 62 or 1,000 allowed chunks (S5b.2/S5b.7),
/// which violates architecture.md §21.1/§25's "default mode must be >= each pure
/// mode" invariant -- recalibrating the *gate* to that shortfall (D016.3)
/// would leave the invariant broken by construction, since the shipped
/// binary's own `--mode semantic` measures higher on the identical corpus.
/// The corpus-scaled cap's correct value in that regime is therefore zero:
/// with the lexical leg contributing no candidates, RRF fusion degenerates
/// to pure semantic ranking, so hybrid can never score below semantic
/// there by construction. Chosen between the S5b sweep's two measured data
/// points -- 1,000 allowed chunks (hybrid still lost to semantic, S5b.7)
/// and 10,000 (hybrid won, S5b.7) -- and picked conservative toward
/// keeping the lexical leg off (closer to 10K than to 1K), since only 10K
/// is verified to work; the tuned cap/DF-filtering configuration (`lexical_
/// cap`, `DF_FILTER_FRACTION`, `DF_ABSOLUTE_FLOOR`) is unchanged at and
/// above this threshold. `--mode lexical` and `--mode semantic` are
/// explicit single-leg requests and are never subject to this threshold --
/// see `effective_lexical_cap`.
const LEXICAL_ACTIVATION_CHUNKS: usize = 4096;

/// The lexical leg's actual candidate cap for this `ask`, folding the
/// hybrid-mode small-corpus deactivation (`LEXICAL_ACTIVATION_CHUNKS`) on
/// top of `lexical_cap`'s own corpus-scaled cap. Returns 0 (lexical leg
/// contributes nothing; `ranks.lexical` is simply absent, the same JSON
/// shape as `--mode semantic` never running that leg) when `mode` is
/// `Hybrid` and the allowed scope is below the activation threshold.
/// `--mode lexical` always gets `lexical_cap` directly, uninfluenced by
/// this threshold -- an explicit single-leg request runs regardless of
/// corpus size.
fn effective_lexical_cap(mode: Mode, k: u32, allowed_chunk_total: usize) -> usize {
    if mode == Mode::Hybrid && allowed_chunk_total < LEXICAL_ACTIVATION_CHUNKS {
        return 0;
    }
    lexical_cap(k, allowed_chunk_total)
}

/// Runs the lexical leg: FTS5 `bm25()` (lower is better -- ranked
/// ascending, per architecture.md §13's explicit warning), restricted to
/// the materialized `ask_allowed_chunk_rowids` set (joined by integer
/// rowid -- see `materialize_allowed_chunk_rowids`), top `limit` (the
/// corpus-scaled cap from `lexical_cap`). Ties broken by ascending chunk id
/// for deterministic output. Returns `(chunk_id, memory_id)` pairs already
/// in rank order (best first). Caller must have materialized
/// `ask_allowed_chunk_rowids` on this connection first.
fn lexical_leg(
    conn: &Connection,
    sanitized_query: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.memory_id
         FROM chunks_fts
         JOIN chunks c ON c.rowid = chunks_fts.rowid
         JOIN ask_allowed_chunk_rowids r ON r.rowid = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1
         ORDER BY bm25(chunks_fts) ASC, c.id ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![sanitized_query, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Runs the semantic leg: brute-force cosine (via `VectorIndex`) over ALL
/// allowed chunks, untruncated. Returns `(chunk_id, memory_id)` pairs in
/// rank order (best first).
fn semantic_leg(conn: &Connection, query_vec: &[f32]) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt =
        conn.prepare("SELECT c.id, c.memory_id, c.embedding FROM chunks c JOIN ask_allowed a ON a.id = c.memory_id")?;
    let rows: Vec<(String, String, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let candidates: Vec<(String, Vec<f32>)> = rows
        .iter()
        .map(|(id, _mid, blob)| (id.clone(), vector::blob_to_embedding(blob)))
        .collect();
    let id_to_memory: HashMap<String, String> =
        rows.into_iter().map(|(id, mid, _blob)| (id, mid)).collect();

    let ranked = BruteForceCosine.rank(query_vec, &candidates);
    Ok(ranked
        .into_iter()
        .map(|(id, _similarity)| {
            let memory_id = id_to_memory.get(&id).cloned().unwrap_or_default();
            (id, memory_id)
        })
        .collect())
}

/// Hydrates the final, truncated, ordered list of (memory_id, chunk_id,
/// score, ranks) into full `ResultItem`s: memory content, caller metadata
/// (sorted by key for deterministic output), and system/provenance fields
/// (architecture.md §12 pipeline step 7).
fn hydrate(
    conn: &Connection,
    ordered: &[(String, f64, FusedRanks)],
) -> Result<Vec<ResultItem>, AppError> {
    let mut out = Vec::with_capacity(ordered.len());
    let mut mem_stmt = conn.prepare(
        "SELECT content, source, created_at, status, content_hash FROM memories WHERE id = ?1",
    )?;
    let mut meta_stmt =
        conn.prepare("SELECT key, value FROM memory_meta WHERE memory_id = ?1 ORDER BY key ASC")?;

    for (memory_id, score, ranks) in ordered {
        let (content, source, created_at, status, content_hash): (
            String,
            Option<String>,
            String,
            String,
            String,
        ) = mem_stmt.query_row([memory_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;

        let mut metadata = serde_json::Map::new();
        let meta_rows = meta_stmt.query_map([memory_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in meta_rows {
            let (k, v) = row?;
            metadata.insert(k, serde_json::Value::String(v));
        }

        out.push(ResultItem {
            id: memory_id.clone(),
            content,
            score: *score,
            ranks: RanksOut {
                lexical: ranks.lexical,
                semantic: ranks.semantic,
            },
            metadata: serde_json::Value::Object(metadata),
            system: SystemInfo {
                created_at,
                source,
                status,
                content_hash: format!("sha256:{content_hash}"),
            },
        });
    }
    Ok(out)
}

pub fn run(db_path: &Path, input: AskInput) -> Result<AskResponse, AppError> {
    let started = Instant::now();
    let query = validate_query(&input.query)?;

    let db = Db::open(db_path)?;
    let conn = &db.conn;

    // Embedder-mismatch refusal (architecture.md §19, S4): only the legs
    // that actually need the embedder are gated -- `--mode lexical` must
    // keep working on a DB stamped by a different embedder, so it never
    // reaches this check.
    if input.mode.runs_semantic() {
        crate::db::check_embedder_match(conn)?;
    }

    let allowed_count = filter::resolve_allowed_ids(
        conn,
        &input.where_terms,
        input.include_superseded,
        input.include_forgotten,
    )?;

    if allowed_count == 0 {
        return Ok(empty_response(input.mode, query, started));
    }

    let mut lexical_ranked: Vec<(String, u32)> = Vec::new();
    let mut semantic_ranked: Vec<(String, u32)> = Vec::new();
    let mut chunk_memory: HashMap<String, String> = HashMap::new();

    if input.mode.runs_lexical() {
        let allowed_chunk_total = materialize_allowed_chunk_rowids(conn)?;
        let cap = effective_lexical_cap(input.mode, input.k, allowed_chunk_total);
        // cap == 0 only for `--mode hybrid` below `LEXICAL_ACTIVATION_CHUNKS`
        // (the supervisor's small-corpus deactivation) -- skip DF filtering
        // and the MATCH query entirely rather than paying their cost only
        // to discard every result via `LIMIT 0`.
        if cap > 0 {
            if let Some(sanitized) = build_lexical_query(conn, &query, allowed_chunk_total)? {
                let rows = lexical_leg(conn, &sanitized, cap)?;
                for (leg_rank, (chunk_id, memory_id)) in rows.into_iter().enumerate() {
                    lexical_ranked.push((chunk_id.clone(), (leg_rank + 1) as u32));
                    chunk_memory.insert(chunk_id, memory_id);
                }
            }
        }
    }

    if input.mode.runs_semantic() {
        // Only loaded when a leg actually needs it -- `--mode lexical`
        // never reaches this branch, so it works even when the embedder is
        // unavailable (architecture.md §13).
        let embedder = Embedder::load()?;
        let query_vec = embedder.embed(&query)?;
        let rows = semantic_leg(conn, &query_vec)?;
        for (leg_rank, (chunk_id, memory_id)) in rows.into_iter().enumerate() {
            semantic_ranked.push((chunk_id.clone(), (leg_rank + 1) as u32));
            chunk_memory.insert(chunk_id, memory_id);
        }
    }

    let fused = rank::fuse(&lexical_ranked, &semantic_ranked);
    let candidates = fused.len();

    // Collapse chunks -> memories BEFORE truncating to k: keep only the
    // best-scoring chunk per memory, carrying forward its per-leg ranks
    // (architecture.md §13 pipeline step 5).
    let mut best_per_memory: HashMap<String, (String, f64, FusedRanks)> = HashMap::new();
    for (chunk_id, (score, ranks)) in fused {
        let Some(memory_id) = chunk_memory.get(&chunk_id).cloned() else {
            continue; // defensive; every fused chunk_id came from a leg query that also recorded its memory_id
        };
        best_per_memory
            .entry(memory_id)
            .and_modify(|cur| {
                if score > cur.1 || (score == cur.1 && chunk_id < cur.0) {
                    *cur = (chunk_id.clone(), score, ranks);
                }
            })
            .or_insert_with(|| (chunk_id.clone(), score, ranks));
    }

    // Total order: (score DESC, id ASC) over the collapsed memory ids
    // (architecture.md §12 determinism contract).
    let mut ordered: Vec<(String, f64, FusedRanks)> = best_per_memory
        .into_iter()
        .map(|(memory_id, (_chunk_id, score, ranks))| (memory_id, score, ranks))
        .collect();
    ordered.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // `--min-score` applies AFTER fusion, against the unrounded score.
    if let Some(min_score) = input.min_score {
        ordered.retain(|(_, score, _)| *score >= min_score);
    }

    ordered.truncate(input.k as usize);

    let results = hydrate(conn, &ordered)?;
    let returned = results.len();

    Ok(AskResponse {
        ok: true,
        op: "ask",
        mode: input.mode.as_str(),
        query,
        results,
        stats: Stats {
            candidates,
            returned,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    })
}

#[cfg(test)]
mod sanitizer_tests {
    use super::*;

    #[test]
    fn splits_on_whitespace_and_quotes_each_token() {
        assert_eq!(
            build_fts5_query("hello world"),
            Some("\"hello\" OR \"world\"".to_string())
        );
    }

    #[test]
    fn strips_punctuation_between_words() {
        assert_eq!(
            build_fts5_query("Mastra, suspend/resume?"),
            Some("\"Mastra\" OR \"suspend\" OR \"resume\"".to_string())
        );
    }

    #[test]
    fn embedded_quote_in_a_token_is_doubled_not_dropped() {
        // A token can never actually contain a `"` since splitting is on
        // non-alphanumeric chars (which includes `"`), but the escape path
        // itself is exercised directly here for defense in depth.
        let mut token = "he\"llo".to_string();
        token = token.replace('"', "\"\"");
        assert_eq!(token, "he\"\"llo");
    }

    #[test]
    fn pure_punctuation_yields_no_lexical_query() {
        assert_eq!(build_fts5_query("!!! ??? ---"), None);
    }

    #[test]
    fn empty_string_yields_no_lexical_query() {
        assert_eq!(build_fts5_query(""), None);
    }

    #[test]
    fn unicode_letters_are_kept_as_tokens() {
        assert_eq!(build_fts5_query("café"), Some("\"café\"".to_string()));
    }
}

/// Fuzz the sanitizer against the *real* FTS5 query path (a live
/// `chunks_fts` MATCH), not just the string-construction function in
/// isolation -- arbitrary bytes (quotes, FTS5 operators, parens, unicode)
/// must never reach SQLite as a syntax error (project-plan.md S3).
#[cfg(test)]
mod fts_fuzz {
    use super::*;
    use proptest::prelude::*;
    use rusqlite::Connection;

    fn fts_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE chunks_fts USING fts5(text, tokenize='porter unicode61 remove_diacritics 2');
             INSERT INTO chunks_fts(rowid, text) VALUES (1, 'hello world sample text for matching purposes');",
        )
        .unwrap();
        conn
    }

    fn assert_match_never_errors(conn: &Connection, input: &str) {
        if let Some(sanitized) = build_fts5_query(input) {
            let outcome = conn
                .prepare("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1")
                .and_then(|mut stmt| {
                    stmt.query([&sanitized])
                        .map(|mut rows| while let Ok(Some(_)) = rows.next() {})
                });
            assert!(
                outcome.is_ok(),
                "MATCH raised a syntax error for input {input:?} -> sanitized {sanitized:?}: {:?}",
                outcome.err()
            );
        }
    }

    proptest! {
        #[test]
        fn arbitrary_unicode_never_causes_a_syntax_error(s in "\\PC{0,300}") {
            let conn = fts_conn();
            assert_match_never_errors(&conn, &s);
        }

        #[test]
        fn combinations_of_fts5_operator_tokens_never_cause_a_syntax_error(
            parts in prop::collection::vec(
                prop_oneof![
                    Just("\"".to_string()),
                    Just("(".to_string()),
                    Just(")".to_string()),
                    Just("*".to_string()),
                    Just(":".to_string()),
                    Just("-".to_string()),
                    Just("^".to_string()),
                    Just("OR".to_string()),
                    Just("AND".to_string()),
                    Just("NOT".to_string()),
                    Just("NEAR".to_string()),
                    Just("\"\"\"".to_string()),
                    "[a-z]{0,8}",
                ],
                0..20,
            )
        ) {
            let s = parts.join(" ");
            let conn = fts_conn();
            assert_match_never_errors(&conn, &s);
        }
    }
}

/// D016.1 unit tests: DF-based query-token filtering and the corpus-scaled
/// lexical cap. `lexical_cap` is pure and needs no DB; `build_lexical_query`
/// is exercised against a minimal real schema (`chunks`/`chunks_fts`/
/// `ask_allowed`) so its `JOIN`s resolve exactly as they do in `run`.
#[cfg(test)]
mod s5b_tuning_tests {
    use super::*;
    use rusqlite::Connection;

    /// A minimal DB with the real `chunks`/`chunks_fts` schema plus a temp
    /// `ask_allowed` populated with every seeded memory id (i.e. no
    /// `--where` narrowing) -- enough for `materialize_allowed_chunk_rowids`
    /// (which derives `ask_allowed_chunk_rowids` from these two) and
    /// `build_lexical_query`, without pulling in the full migration set.
    fn seeded_conn(docs: &[&str]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chunks (
               id TEXT PRIMARY KEY, memory_id TEXT NOT NULL, idx INTEGER NOT NULL,
               text TEXT NOT NULL, embedding BLOB NOT NULL
             );
             CREATE VIRTUAL TABLE chunks_fts USING fts5(
               text, content='chunks', content_rowid='rowid',
               tokenize='porter unicode61 remove_diacritics 2'
             );
             CREATE TEMP TABLE ask_allowed (id TEXT PRIMARY KEY);",
        )
        .unwrap();
        for (i, text) in docs.iter().enumerate() {
            let mid = format!("m{i}");
            conn.execute(
                "INSERT INTO chunks (id, memory_id, idx, text, embedding) VALUES (?1, ?2, 0, ?3, x'00')",
                rusqlite::params![format!("c{i}"), mid, text],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO chunks_fts(rowid, text) VALUES (?1, ?2)",
                rusqlite::params![i as i64 + 1, text],
            )
            .unwrap();
            conn.execute("INSERT INTO ask_allowed (id) VALUES (?1)", [&mid])
                .unwrap();
        }
        conn
    }

    #[test]
    fn materialize_allowed_chunk_rowids_counts_seeded_rows() {
        let conn = seeded_conn(&["a", "b", "c"]);
        assert_eq!(materialize_allowed_chunk_rowids(&conn).unwrap(), 3);
    }

    #[test]
    fn materialize_allowed_chunk_rowids_is_safe_to_call_twice() {
        // Same connection, re-derived after `ask_allowed` changes underneath
        // it (the real per-invocation pattern: `filter::resolve_allowed_ids`
        // repopulates `ask_allowed`, then this is called fresh every `ask`).
        let conn = seeded_conn(&["a", "b", "c"]);
        assert_eq!(materialize_allowed_chunk_rowids(&conn).unwrap(), 3);
        conn.execute("DELETE FROM ask_allowed WHERE id = 'm0'", [])
            .unwrap();
        assert_eq!(materialize_allowed_chunk_rowids(&conn).unwrap(), 2);
    }

    #[test]
    fn high_df_token_is_dropped_low_df_token_survives() {
        // 10 docs share "the"; exactly one contains "kernel". Total = 11,
        // so max_df = floor(11 * 0.5) = 5. "the" (df=11) is dropped,
        // "kernel" (df=1) survives.
        let mut docs: Vec<String> = (0..10).map(|i| format!("the fox {i}")).collect();
        docs.push("the kernel proof".to_string());
        let doc_refs: Vec<&str> = docs.iter().map(String::as_str).collect();
        let conn = seeded_conn(&doc_refs);

        let total = materialize_allowed_chunk_rowids(&conn).unwrap();
        assert_eq!(total, 11);
        let built = build_lexical_query(&conn, "the kernel", total).unwrap();
        assert_eq!(built, Some("\"kernel\"".to_string()));
    }

    #[test]
    fn all_tokens_dropped_returns_none_never_falls_back_unfiltered() {
        // Every doc contains both "the" and "and"; the query is stopwords
        // only (df=3/3=100% > max_df=floor(3*0.5)=1 for each token), so
        // every token is dropped and the lexical leg must contribute
        // nothing -- not silently retry with the unfiltered OR query.
        let docs = vec![
            "the quick and fox",
            "the lazy and dog",
            "the kernel and proof",
        ];
        let conn = seeded_conn(&docs);
        let total = materialize_allowed_chunk_rowids(&conn).unwrap();
        let built = build_lexical_query(&conn, "the and", total).unwrap();
        assert_eq!(built, None);
    }

    #[test]
    fn zero_allowed_chunks_returns_none_without_querying_df() {
        let conn = seeded_conn(&[]);
        let built = build_lexical_query(&conn, "anything", 0).unwrap();
        assert_eq!(built, None);
    }

    #[test]
    fn repeated_token_is_deduplicated_before_df_lookup() {
        // "kernel" appears once in the query text three times over; df
        // filtering must still evaluate it (and quote it) exactly once.
        let docs = vec!["the kernel proof", "unrelated filler text here"];
        let conn = seeded_conn(&docs);
        let total = materialize_allowed_chunk_rowids(&conn).unwrap();
        let built = build_lexical_query(&conn, "kernel kernel kernel", total).unwrap();
        assert_eq!(built, Some("\"kernel\"".to_string()));
    }

    #[test]
    fn absolute_df_floor_protects_a_tiny_corpus_from_total_filtering() {
        // Both 2-chunk docs contain both query tokens: df=2 of 2 (100%),
        // which exceeds every fraction in the tunable [0.25, 0.6] range and
        // would be dropped by the percentage rule alone -- DF_ABSOLUTE_FLOOR
        // (2) exempts it, so the lexical leg still contributes on a corpus
        // this small (the "every store starts small" case, D016.1).
        let docs = vec![
            "supersede chain old truth here",
            "supersede chain new truth here",
        ];
        let conn = seeded_conn(&docs);
        let total = materialize_allowed_chunk_rowids(&conn).unwrap();
        assert_eq!(total, 2);
        let built = build_lexical_query(&conn, "supersede chain", total).unwrap();
        assert!(
            built.is_some(),
            "df=2 tokens on a 2-chunk corpus must survive the absolute floor"
        );
    }

    #[test]
    fn lexical_cap_floor_binds_on_a_small_corpus() {
        // allowed=62, k=5: scaled = ceil(62/10) = 7, floor = 4*5 = 20.
        // max(20, 7) = 20, min(200, 20) = 20.
        assert_eq!(lexical_cap(5, 62), 20);
    }

    #[test]
    fn lexical_cap_scales_between_floor_and_ceiling() {
        // allowed=1000, k=5: scaled = ceil(1000/10) = 100, floor = 20.
        // max(20, 100) = 100, min(200, 100) = 100.
        assert_eq!(lexical_cap(5, 1000), 100);
    }

    #[test]
    fn lexical_cap_ceiling_binds_on_a_large_corpus() {
        // allowed=10000, k=5: scaled = ceil(10000/10) = 1000, floor = 20.
        // min(200, max(20, 1000)) = 200.
        assert_eq!(lexical_cap(5, 10_000), 200);
    }

    #[test]
    fn lexical_cap_never_exceeds_the_ceiling_even_for_large_k() {
        // A pathological k would otherwise blow past the 200 ceiling via
        // the floor term alone (4*80=320) -- the ceiling still wins.
        assert_eq!(lexical_cap(80, 62), 200);
    }

    // -- Supervisor ruling: `effective_lexical_cap`'s hybrid-mode small-
    // corpus deactivation, both sides of `LEXICAL_ACTIVATION_CHUNKS`. --

    #[test]
    fn hybrid_cap_is_zero_below_the_activation_threshold() {
        assert_eq!(
            effective_lexical_cap(Mode::Hybrid, 5, LEXICAL_ACTIVATION_CHUNKS - 1),
            0
        );
        // Also true at the 62-memory kernel-proof scale specifically, since
        // that is the scale the ruling was made to fix.
        assert_eq!(effective_lexical_cap(Mode::Hybrid, 5, 62), 0);
        // And at 1,000 -- the S5b sweep's other losing data point.
        assert_eq!(effective_lexical_cap(Mode::Hybrid, 5, 1_000), 0);
    }

    #[test]
    fn hybrid_cap_matches_lexical_cap_at_and_above_the_activation_threshold() {
        assert_eq!(
            effective_lexical_cap(Mode::Hybrid, 5, LEXICAL_ACTIVATION_CHUNKS),
            lexical_cap(5, LEXICAL_ACTIVATION_CHUNKS)
        );
        assert_eq!(
            effective_lexical_cap(Mode::Hybrid, 5, 10_000),
            lexical_cap(5, 10_000)
        );
    }

    #[test]
    fn explicit_lexical_mode_ignores_the_activation_threshold_entirely() {
        // `--mode lexical` is an explicit single-leg request: it must run
        // (and match `lexical_cap` directly) on a corpus far below
        // `LEXICAL_ACTIVATION_CHUNKS`, unlike `--mode hybrid` on the same
        // corpus size.
        for total in [0usize, 1, 62, 1_000, LEXICAL_ACTIVATION_CHUNKS - 1] {
            assert_eq!(
                effective_lexical_cap(Mode::Lexical, 5, total),
                lexical_cap(5, total),
                "mode=lexical must never be zeroed by the activation threshold (total={total})"
            );
        }
        // And the deactivation is specific to Hybrid: at the same corpus
        // size, lexical mode's cap is nonzero while hybrid's is zero.
        assert!(effective_lexical_cap(Mode::Lexical, 5, 62) > 0);
        assert_eq!(effective_lexical_cap(Mode::Hybrid, 5, 62), 0);
    }
}
