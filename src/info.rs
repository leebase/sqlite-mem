//! `info` verb -- basic form (architecture.md §18; `--verify` is S4 scope).

use crate::db::Db;
use crate::error::AppError;
use crate::paths;
use serde::Serialize;

#[derive(Serialize)]
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

pub fn run(db_path: &std::path::Path) -> Result<InfoResponse, AppError> {
    let db = Db::open(db_path)?;
    let conn = &db.conn;

    let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    let embedder_id: String = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| crate::embed::EMBEDDER_ID.to_string());
    let embedder_dims: i64 = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_dims'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(crate::embed::EMBEDDER_DIMS as i64);

    let count_for = |status: &str| -> Result<i64, AppError> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE status = ?1",
            [status],
            |r| r.get(0),
        )?)
    };

    let counts = StatusCounts {
        active: count_for("active")?,
        superseded: count_for("superseded")?,
        forgotten: count_for("forgotten")?,
    };

    let chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;

    let db_size_bytes = std::fs::metadata(&db.path).map(|m| m.len()).unwrap_or(0);

    Ok(InfoResponse {
        ok: true,
        op: "info",
        path: paths::resolved_path_for_display(&db.path),
        schema_version,
        embedder: EmbedderInfo {
            id: embedder_id,
            dims: embedder_dims,
        },
        counts,
        chunks,
        db_size_bytes,
    })
}
