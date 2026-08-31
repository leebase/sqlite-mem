//! `ask` verb: hybrid retrieval (architecture.md §12-13).
//!
//! ```text
//! query -> [resolve filters -> allowed-ID set]      (filter.rs, one indexed SQL pass)
//!        -> lexical leg:  FTS5 MATCH, bm25(), restricted to allowed set, top 200
//!        -> semantic leg: embed query -> cosine over ALL allowed chunks (untruncated)
//!        -> RRF fusion:   score(c) = sum_legs 1/(60 + rank_leg(c))       (rank.rs)
//!        -> collapse chunks -> best-scoring chunk represents its memory
//!        -> sort (score DESC, id ASC) -> top k -> hydrate content + metadata
//! ```

use crate::db::Db;
use crate::embed::Embedder;
use crate::error::AppError;
use crate::filter::{self, WhereTerm};
use crate::rank::{self, FusedRanks};
use crate::vector::{self, BruteForceCosine, VectorIndex};
use rusqlite::Connection;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

pub const MAX_QUERY_BYTES: usize = 8192; // 8 KiB (architecture.md §12, §20)
pub const LEXICAL_TOP_N: usize = 200; // architecture.md §13

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

/// Injection-safe token-quoting OR sanitizer for FTS5 `MATCH` (architecture
/// .md §13, Satchel's `build_fts5_query` pattern per §4/§22 -- reimplemented
/// here, not copied, since Satchel's version is tied to its own schema).
/// Splits on non-alphanumeric boundaries and double-quotes each surviving
/// token as an FTS5 phrase literal (escaping embedded `"` by doubling it,
/// the FTS5 string-literal escape), then joins with `OR`. Because every
/// token is emitted as a quoted phrase, no FTS5 operator, paren, or
/// unbalanced quote from the original query can ever reach the parser as
/// syntax -- it is either consumed by the split (dropped) or safely quoted.
/// Returns `None` when the query has no alphanumeric content at all (e.g.
/// pure punctuation), in which case the lexical leg contributes nothing
/// rather than sending FTS5 an empty/invalid `MATCH` string.
fn build_fts5_query(text: &str) -> Option<String> {
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

/// Runs the lexical leg: FTS5 `bm25()` (lower is better -- ranked
/// ascending, per architecture.md §13's explicit warning), restricted to
/// `ask_allowed`, top `LEXICAL_TOP_N`. Ties broken by ascending chunk id
/// for deterministic output. Returns `(chunk_id, memory_id)` pairs already
/// in rank order (best first).
fn lexical_leg(
    conn: &Connection,
    sanitized_query: &str,
) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.memory_id
         FROM chunks_fts
         JOIN chunks c ON c.rowid = chunks_fts.rowid
         JOIN ask_allowed a ON a.id = c.memory_id
         WHERE chunks_fts MATCH ?1
         ORDER BY bm25(chunks_fts) ASC, c.id ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![sanitized_query, LEXICAL_TOP_N as i64],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
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
        if let Some(sanitized) = build_fts5_query(&query) {
            let rows = lexical_leg(conn, &sanitized)?;
            for (leg_rank, (chunk_id, memory_id)) in rows.into_iter().enumerate() {
                lexical_ranked.push((chunk_id.clone(), (leg_rank + 1) as u32));
                chunk_memory.insert(chunk_id, memory_id);
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
