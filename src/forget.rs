//! `forget` verb: default forget, `--purge`, `--restore` (architecture.md
//! §15). Never gated by embedder identity (architecture.md §19: `forget`
//! keeps working regardless of embedder mismatch -- it never touches
//! embeddings).
//!
//! All three modes are all-or-nothing per invocation: every id must exist
//! in `memories` (any status) or the whole call fails with exit 4 and
//! nothing changes (checked before the transaction opens, and the mutating
//! work itself runs inside one `IMMEDIATE` transaction so a mid-batch SQL
//! error also rolls back everything).

use crate::db::Db;
use crate::error::AppError;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgetMode {
    Forget,
    Purge,
    Restore,
}

impl ForgetMode {
    fn as_str(self) -> &'static str {
        match self {
            ForgetMode::Forget => "forget",
            ForgetMode::Purge => "purge",
            ForgetMode::Restore => "restore",
        }
    }
}

pub struct ForgetInput {
    pub ids: Vec<String>,
    pub mode: ForgetMode,
}

#[derive(Serialize)]
struct ForgetOutcome {
    id: String,
    /// The memory's resulting status: `"forgotten"`, `"active"`,
    /// `"superseded"`, or `"purged"` (purge has no post-state row to
    /// report a status from, so it reports the literal outcome instead).
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    forgotten_at: Option<String>,
    /// `false` when the id was already in the requested end state and this
    /// call was a no-op (idempotent retry-safety, matching `save`'s dedup
    /// and supersession precedent).
    changed: bool,
}

#[derive(Serialize)]
pub struct ForgetResponse {
    ok: bool,
    op: &'static str,
    mode: &'static str,
    /// `true` only for `--purge` (architecture.md §15: "Purge is the only
    /// destructive operation in the product and says so in its JSON
    /// response").
    destructive: bool,
    results: Vec<ForgetOutcome>,
    count: usize,
}

/// Returns the ids in `ids` that do not exist in `memories` at all
/// (any status). Used for the all-or-nothing not_found refusal.
fn missing_ids(conn: &Connection, ids: &[String]) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT 1 FROM memories WHERE id = ?1")?;
    let mut missing = Vec::new();
    for id in ids {
        if !stmt.exists(params![id])? {
            missing.push(id.clone());
        }
    }
    Ok(missing)
}

pub fn run(db_path: &std::path::Path, input: ForgetInput) -> Result<ForgetResponse, AppError> {
    let db = Db::open(db_path)?;
    let conn = &db.conn;

    let missing = missing_ids(conn, &input.ids)?;
    if !missing.is_empty() {
        return Err(
            AppError::not_found(format!("unknown memory id(s): {}", missing.join(", "))).with_hint(
                "no changes were made -- forget/purge/restore are all-or-nothing per invocation",
            ),
        );
    }

    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = match input.mode {
        ForgetMode::Forget => forget_ids(conn, &input.ids),
        ForgetMode::Restore => restore_ids(conn, &input.ids),
        ForgetMode::Purge => purge_ids(conn, &input.ids),
    };

    let results = match result {
        Ok(r) => {
            conn.execute_batch("COMMIT;")?;
            r
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e);
        }
    };

    Ok(ForgetResponse {
        ok: true,
        op: "forget",
        mode: input.mode.as_str(),
        destructive: matches!(input.mode, ForgetMode::Purge),
        count: results.len(),
        results,
    })
}

/// Marks each id `forgotten` (architecture.md §15). Idempotent: an id
/// already `forgotten` is left exactly as-is (its original `forgotten_at`
/// is not overwritten by a repeated call).
fn forget_ids(conn: &Connection, ids: &[String]) -> Result<Vec<ForgetOutcome>, AppError> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut sel = conn.prepare("SELECT status, forgotten_at FROM memories WHERE id = ?1")?;
    let mut upd = conn.prepare(
        "UPDATE memories SET status = 'forgotten', forgotten_at = ?2 WHERE id = ?1 AND status != 'forgotten'",
    )?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let (prev_status, prev_forgotten_at): (String, Option<String>) =
            sel.query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if prev_status == "forgotten" {
            out.push(ForgetOutcome {
                id: id.clone(),
                status: "forgotten".to_string(),
                forgotten_at: prev_forgotten_at,
                changed: false,
            });
        } else {
            upd.execute(params![id, now])?;
            out.push(ForgetOutcome {
                id: id.clone(),
                status: "forgotten".to_string(),
                forgotten_at: Some(now.clone()),
                changed: true,
            });
        }
    }
    Ok(out)
}

/// Undoes `forget`. A memory that carries a non-null `superseded_by` was,
/// at some point, superseded -- that column is a permanent marker never
/// cleared by anything except a purge of its target (see `purge_ids`), so
/// its presence is how `restore` distinguishes "this was superseded, then
/// forgotten" from "this was plain active, then forgotten" without a
/// dedicated pre-forget-status column (architecture.md §15: "a memory
/// that is superseded stays superseded -- restore only undoes forget").
/// An id that isn't currently `forgotten` is left untouched (no-op,
/// idempotent retry-safety).
fn restore_ids(conn: &Connection, ids: &[String]) -> Result<Vec<ForgetOutcome>, AppError> {
    let mut sel = conn.prepare("SELECT status, superseded_by FROM memories WHERE id = ?1")?;
    let mut upd = conn.prepare(
        "UPDATE memories SET status = ?2, forgotten_at = NULL WHERE id = ?1 AND status = 'forgotten'",
    )?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let (prev_status, superseded_by): (String, Option<String>) =
            sel.query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if prev_status != "forgotten" {
            out.push(ForgetOutcome {
                id: id.clone(),
                status: prev_status,
                forgotten_at: None,
                changed: false,
            });
            continue;
        }
        let restored_status = if superseded_by.is_some() {
            "superseded"
        } else {
            "active"
        };
        upd.execute(params![id, restored_status])?;
        out.push(ForgetOutcome {
            id: id.clone(),
            status: restored_status.to_string(),
            forgotten_at: None,
            changed: true,
        });
    }
    Ok(out)
}

/// Hard-deletes memory + chunks + FTS rows + metadata for each id, in the
/// caller's already-open transaction (architecture.md §15). Chunks are
/// deleted with an explicit `DELETE` (not left to the `ON DELETE CASCADE`
/// from the `memories` delete) specifically so the `chunks_ad` trigger
/// fires per row and keeps `chunks_fts` in sync -- relying on the cascade
/// alone is unverified/riskier territory this sprint doesn't need to
/// enter. Any other memory's `superseded_by` pointing at a purged id is
/// nulled first so the delete can't trip the `memories(id)` foreign key
/// (a purged memory can no longer be "pointed at" from history; this also
/// keeps `restore`'s superseded-by-presence check well-defined for
/// whatever remains).
fn purge_ids(conn: &Connection, ids: &[String]) -> Result<Vec<ForgetOutcome>, AppError> {
    let mut null_refs =
        conn.prepare("UPDATE memories SET superseded_by = NULL WHERE superseded_by = ?1")?;
    let mut del_chunks = conn.prepare("DELETE FROM chunks WHERE memory_id = ?1")?;
    let mut del_meta = conn.prepare("DELETE FROM memory_meta WHERE memory_id = ?1")?;
    let mut del_mem = conn.prepare("DELETE FROM memories WHERE id = ?1")?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        null_refs.execute(params![id])?;
        del_chunks.execute(params![id])?;
        del_meta.execute(params![id])?;
        del_mem.execute(params![id])?;
        out.push(ForgetOutcome {
            id: id.clone(),
            status: "purged".to_string(),
            forgotten_at: None,
            changed: true,
        });
    }
    Ok(out)
}
