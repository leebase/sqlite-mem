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

/// Up to how many memories `--verify`'s content_hash check recomputes
/// sha256 for (architecture.md §18: "hash spot-checks... up to 100 sampled
/// memories"). Deterministic sample (ascending id), not random -- so two
/// consecutive `--verify` runs against an unchanged db agree.
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

fn embedder_info(conn: &Connection) -> EmbedderInfo {
    let id: String = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| crate::embed::EMBEDDER_ID.to_string());
    let dims: i64 = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_dims'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(crate::embed::EMBEDDER_DIMS as i64);
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

/// Counts must agree, and FTS5's own `'integrity-check'` command (which,
/// for an external-content table, re-tokenizes every content-table row and
/// compares it against the index) must not report a mismatch.
fn check_fts_consistency(conn: &Connection) -> Result<CheckResult, AppError> {
    let chunks_count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let fts_count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks_fts", [], |r| r.get(0))?;
    if chunks_count != fts_count {
        return Ok(CheckResult {
            pass: false,
            detail: format!("chunks has {chunks_count} row(s) but chunks_fts has {fts_count}"),
        });
    }
    match conn.execute(
        "INSERT INTO chunks_fts(chunks_fts) VALUES('integrity-check')",
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
    let mut stmt =
        conn.prepare("SELECT id, content, content_hash FROM memories ORDER BY id LIMIT ?1")?;
    let rows = stmt.query_map([CONTENT_HASH_SAMPLE_LIMIT], |r| {
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
    let embedding_dims = check_embedding_dims(conn, embedder.dims)?;
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
