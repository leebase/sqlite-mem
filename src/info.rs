//! `info` verb: basic form (architecture.md §18) and `--verify` (S4):
//! `PRAGMA integrity_check`, an FTS-vs-chunks consistency audit, an
//! embedding-dimension audit (every chunk), and a content_hash spot-check
//! (up to 100 sampled memories). Neither form calls
//! `db::check_embedder_match` -- `info` reports on a DB regardless of
//! which embedder wrote it (architecture.md §19).

use crate::db::Db;
use crate::error::AppError;
use crate::paths;
use rusqlite::Connection;
use serde::Serialize;

/// Target sample size for `--verify`'s content_hash check (architecture.md
/// §18: "hash spot-checks... up to 100 sampled memories"), used to derive a
/// stride rather than a hard `LIMIT`. S6 audit F11: sampling the 100
/// *oldest* memories (`ORDER BY id LIMIT 100`) meant recent tampering was
/// never checked at all on any db past ~100 memories -- `check_content_hash`
/// below instead strides evenly across the whole `id`-ordered set, anchored
/// so the single *newest* memory is always included in the sample
/// regardless of stride math. Deterministic (id-ordered, no randomness), so
/// two consecutive `--verify` runs against an unchanged db agree.
const CONTENT_HASH_SAMPLE_LIMIT: i64 = 100;

#[derive(Serialize, Clone)]
struct EmbedderInfo {
    id: String,
    dims: i64,
}

#[derive(Serialize)]
struct StatusCounts {
    active: i64,
    superseded: i64,
    forgotten: i64,
}

#[derive(Serialize)]
pub struct InfoResponse {
    ok: bool,
    op: &'static str,
    path: String,
    schema_version: i64,
    embedder: EmbedderInfo,
    counts: StatusCounts,
    chunks: i64,
    db_size_bytes: u64,
}

/// The raw, parsed `db_info.embedder_dims` value, or `None` if the row is
/// missing or its value isn't a valid integer. Split out from
/// `embedder_info` so `run_verify`'s dims-audit can tell "missing/
/// unparseable" apart from "parsed to some value" (S6 audit info item (b))
/// instead of both silently collapsing into the same 384 fallback.
fn raw_embedder_dims(conn: &Connection) -> Option<i64> {
    conn.query_row(
        "SELECT value FROM db_info WHERE key = 'embedder_dims'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| s.parse().ok())
}

/// Descriptive embedder info for the plain (non-`--verify`) `info` response
/// and for display alongside `--verify`'s own `checks`. Falls back to this
/// binary's own embedder dims when `db_info` doesn't have a usable value --
/// a reasonable default for a purely informational field, unlike `--verify`'s
/// dims-audit (`run_verify`), which must fail loudly instead of silently
/// substituting this same fallback (S6 audit info item (b)).
fn embedder_info(conn: &Connection) -> EmbedderInfo {
    let id: String = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| crate::embed::EMBEDDER_ID.to_string());
    let dims = raw_embedder_dims(conn).unwrap_or(crate::embed::EMBEDDER_DIMS as i64);
    EmbedderInfo { id, dims }
}

fn status_counts(conn: &Connection) -> Result<StatusCounts, AppError> {
    let count_for = |status: &str| -> Result<i64, AppError> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE status = ?1",
            [status],
            |r| r.get(0),
        )?)
    };
    Ok(StatusCounts {
        active: count_for("active")?,
        superseded: count_for("superseded")?,
        forgotten: count_for("forgotten")?,
    })
}

pub fn run(db_path: &std::path::Path) -> Result<InfoResponse, AppError> {
    let db = Db::open(db_path)?;
    let conn = &db.conn;

    let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let embedder = embedder_info(conn);
    let counts = status_counts(conn)?;
    let chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let db_size_bytes = std::fs::metadata(&db.path).map(|m| m.len()).unwrap_or(0);

    Ok(InfoResponse {
        ok: true,
        op: "info",
        path: paths::resolved_path_for_display(&db.path),
        schema_version,
        embedder,
        counts,
        chunks,
        db_size_bytes,
    })
}

// --- `info --verify` --------------------------------------------------

#[derive(Serialize)]
struct CheckResult {
    pass: bool,
    detail: String,
}

#[derive(Serialize)]
struct Checks {
    integrity_check: CheckResult,
    fts_consistency: CheckResult,
    embedding_dims: CheckResult,
    content_hash: CheckResult,
}

impl Checks {
    /// Names of the checks that failed, in declaration order -- used to
    /// build `error.message` on a failed verify (architecture.md §18,
    /// amended: "error.code=\"integrity_failed\" (message naming the
    /// failed checks)").
    fn failed_names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.integrity_check.pass {
            out.push("integrity_check");
        }
        if !self.fts_consistency.pass {
            out.push("fts_consistency");
        }
        if !self.embedding_dims.pass {
            out.push("embedding_dims");
        }
        if !self.content_hash.pass {
            out.push("content_hash");
        }
        out
    }
}

/// Mirrors `output::ErrorField`'s shape exactly (architecture.md §17's
/// `{code, message, hint}` error object) -- not reused directly because
/// that struct is private to `output.rs` and this envelope needs to
/// carry it alongside a `checks` object that the plain `AppError` path
/// has no room for.
#[derive(Serialize)]
struct ErrorField {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    /// Architecture.md §18 (amended): "every non-zero exit pairs with
    /// `ok:false`" -- `false` exactly when any check below failed, in
    /// which case `error` is `Some` and `main` exits 7; otherwise `error`
    /// is `None` and `main` exits 0.
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorField>,
    op: &'static str,
    verify: bool,
    path: String,
    schema_version: i64,
    embedder: EmbedderInfo,
    counts: StatusCounts,
    chunks: i64,
    db_size_bytes: u64,
    checks: Checks,
}

fn check_integrity(conn: &Connection) -> Result<CheckResult, AppError> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() == 1 && rows[0] == "ok" {
        Ok(CheckResult {
            pass: true,
            detail: "ok".to_string(),
        })
    } else {
        Ok(CheckResult {
            pass: false,
            detail: rows.join("; "),
        })
    }
}

/// Runs FTS5's own `'integrity-check'` command, in its rank-1 form, against
/// `chunks_fts`. `chunks_fts` is an external-content table (`content=
/// 'chunks'`), so this re-tokenizes every row `chunks` itself holds and
/// compares the result against the shadow index, raising `SQLITE_CORRUPT_
/// VTAB` the moment the two diverge.
///
/// S6 audit F1: the bare `INSERT INTO chunks_fts(chunks_fts) VALUES(
/// 'integrity-check')` form this used to run is unfalsifiable -- for an
/// external-content table it always succeeds regardless of index content,
/// so this check never actually caught anything. The `(chunks_fts, rank)`
/// form with `rank = 1` is the one FTS5 documents as doing the real
/// re-tokenize-and-compare work (auditor-verified to return
/// `SQLITE_CORRUPT_VTAB` on real desyncs: a wiped/edited index, a dropped
/// sync trigger followed by a content edit, or a single deleted index row).
/// The previous `COUNT(*) FROM chunks` vs `COUNT(*) FROM chunks_fts`
/// comparison above this comment is gone too, for the same reason: on an
/// external-content table, `SELECT COUNT(*) FROM chunks_fts` reads through
/// to `chunks` itself rather than the shadow index, so the two counts are
/// equal by construction and that comparison could never fail either.
fn check_fts_consistency(conn: &Connection) -> Result<CheckResult, AppError> {
    let chunks_count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    match conn.execute(
        "INSERT INTO chunks_fts(chunks_fts, rank) VALUES('integrity-check', 1)",
        [],
    ) {
        Ok(_) => Ok(CheckResult {
            pass: true,
            detail: format!("{chunks_count} chunk(s) in sync with the fts index"),
        }),
        Err(e) => Ok(CheckResult {
            pass: false,
            detail: format!("fts5 integrity-check failed: {e}"),
        }),
    }
}

fn check_embedding_dims(conn: &Connection, dims: i64) -> Result<CheckResult, AppError> {
    let expected = dims * 4;
    let mut stmt = conn.prepare("SELECT id, length(embedding) FROM chunks")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut total = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for row in rows {
        let (id, len) = row?;
        total += 1;
        if len != expected {
            bad.push(id);
        }
    }
    if bad.is_empty() {
        Ok(CheckResult {
            pass: true,
            detail: format!("{total} chunk(s), all {expected}-byte embeddings"),
        })
    } else {
        let sample: Vec<&String> = bad.iter().take(5).collect();
        Ok(CheckResult {
            pass: false,
            detail: format!(
                "{} of {total} chunk(s) have an embedding length != {expected} bytes (e.g. {sample:?})",
                bad.len()
            ),
        })
    }
}

fn check_content_hash(conn: &Connection) -> Result<CheckResult, AppError> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    if total == 0 {
        return Ok(CheckResult {
            pass: true,
            detail: "0 memory content_hash(es) verified".to_string(),
        });
    }
    // Stride, not `LIMIT` (S6 audit F11): a fixed `ORDER BY id LIMIT 100`
    // only ever sampled the 100 OLDEST memories (`id` is a ULID, so
    // ascending order is chronological), which meant tampering with recent
    // memories on any db past ~100 rows was never checked at all. Anchoring
    // the stride from the NEWEST row (`total - 1 - rn`, `rn` numbered
    // oldest-first) guarantees the single newest memory always lands in the
    // sample -- `(total - 1) - (total - 1) = 0`, divisible by any stride --
    // while still spreading the rest of the sample evenly back through the
    // whole id-ordered set, deterministically (same stride, same rows,
    // every run against an unchanged db).
    let stride = ((total as f64) / (CONTENT_HASH_SAMPLE_LIMIT as f64))
        .ceil()
        .max(1.0) as i64;
    let mut stmt = conn.prepare(
        "SELECT id, content, content_hash FROM (
           SELECT id, content, content_hash,
                  ROW_NUMBER() OVER (ORDER BY id ASC) - 1 AS rn,
                  COUNT(*) OVER () AS total
           FROM memories
         )
         WHERE (total - 1 - rn) % ?1 = 0
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([stride], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut sampled = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for row in rows {
        let (id, content, stored_hash) = row?;
        sampled += 1;
        if crate::save::sha256_hex(&content) != stored_hash {
            bad.push(id);
        }
    }
    if bad.is_empty() {
        Ok(CheckResult {
            pass: true,
            detail: format!("{sampled} memory content_hash(es) verified"),
        })
    } else {
        Ok(CheckResult {
            pass: false,
            detail: format!(
                "{} of {sampled} sampled memory content_hash(es) do not match their recomputed sha256: {bad:?}",
                bad.len()
            ),
        })
    }
}

/// Runs every `--verify` check and returns `(response, passed)`; `main`
/// emits `response` (via `output::emit`, since it may carry `ok:false`)
/// and exits 0 or 7 based on `passed` -- `response.ok`/`response.error`
/// already agree with that exit code (architecture.md §18, amended).
pub fn run_verify(db_path: &std::path::Path) -> Result<(VerifyResponse, bool), AppError> {
    let db = Db::open(db_path)?;
    let conn = &db.conn;

    let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let embedder = embedder_info(conn);
    let counts = status_counts(conn)?;
    let chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let db_size_bytes = std::fs::metadata(&db.path).map(|m| m.len()).unwrap_or(0);

    let integrity_check = check_integrity(conn)?;
    let fts_consistency = check_fts_consistency(conn)?;
    // Unlike `embedder.dims` above (a descriptive field that falls back to
    // this binary's own dims when db_info is missing/unparseable), the
    // dims-audit itself must FAIL on that case rather than silently
    // substituting 384 and auditing against a value the db never actually
    // recorded (S6 audit info item (b)).
    let embedding_dims = match raw_embedder_dims(conn) {
        Some(dims) => check_embedding_dims(conn, dims)?,
        None => CheckResult {
            pass: false,
            detail: "db_info.embedder_dims is missing or not a valid integer -- cannot audit \
                     embedding lengths against an unknown expected size"
                .to_string(),
        },
    };
    let content_hash = check_content_hash(conn)?;

    let checks = Checks {
        integrity_check,
        fts_consistency,
        embedding_dims,
        content_hash,
    };
    let failed = checks.failed_names();
    let passed = failed.is_empty();

    let error = if passed {
        None
    } else {
        Some(ErrorField {
            code: "integrity_failed",
            message: format!("integrity check(s) failed: {}", failed.join(", ")),
            hint: Some(
                "inspect the `checks` object for detail; recover by restoring the most recent \
                 .bak, or by exporting content with sqlite3 and re-saving it (architecture.md §19)"
                    .to_string(),
            ),
        })
    };

    let response = VerifyResponse {
        ok: passed,
        error,
        op: "info",
        verify: true,
        path: paths::resolved_path_for_display(&db.path),
        schema_version,
        embedder,
        counts,
        chunks,
        db_size_bytes,
        checks,
    };
    Ok((response, passed))
}
