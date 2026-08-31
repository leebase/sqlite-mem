//! `reindex` verb (architecture.md §19): re-embeds every chunk with the
//! binary's current embedder, inside one transaction, after a timestamped
//! `.bak` copy of the db file (reusing `db::backup_file`, the same helper
//! the migration runner uses). Works even when `db_info`'s recorded
//! embedder differs from this binary's own -- that mismatch is exactly the
//! case `reindex` exists to fix, so unlike `save`/`ask --mode
//! hybrid|semantic` it never calls `db::check_embedder_match`.

use crate::db::{self, Db};
use crate::embed::Embedder;
use crate::error::AppError;
use rusqlite::params;
use serde::Serialize;

#[derive(Serialize)]
struct EmbedderInfo {
    id: String,
    dims: usize,
}

#[derive(Serialize)]
pub struct ReindexResponse {
    ok: bool,
    op: &'static str,
    chunks_reindexed: usize,
    /// Path of the pre-reindex `.bak` copy, or `""` when there was no
    /// pre-existing db file to back up (a `reindex` against a path that
    /// doesn't exist yet just creates an empty, current-embedder db --
    /// there is nothing to re-embed and nothing to back up).
    backup: String,
    previous_embedder: EmbedderInfo,
    embedder: EmbedderInfo,
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

pub fn run(db_path: &std::path::Path) -> Result<ReindexResponse, AppError> {
    // .bak first, before the db is opened/migrated/mutated at all --
    // mirrors the migration runner's ordering (architecture.md §19).
    let backup_path = if db_path.exists() {
        Some(db::backup_file(db_path)?)
    } else {
        None
    };

    let db = Db::open(db_path)?;
    let conn = &db.conn;

    let previous_embedder_id: String = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| crate::embed::EMBEDDER_ID.to_string());
    let previous_embedder_dims: usize = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_dims'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(crate::embed::EMBEDDER_DIMS);

    // Loaded once, up front, outside the transaction -- model load
    // failures should not open a write transaction at all.
    let embedder = Embedder::load()?;

    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = (|| -> Result<usize, AppError> {
        let mut stmt = conn.prepare("SELECT id, text FROM chunks ORDER BY id")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut upd = conn.prepare("UPDATE chunks SET embedding = ?2 WHERE id = ?1")?;
        for (chunk_id, text) in &rows {
            let vector = embedder.embed(text)?;
            if vector.len() != embedder.dims() {
                return Err(AppError::database(
                    "embedding_dims_mismatch",
                    format!(
                        "embedder returned {} dims, expected {}",
                        vector.len(),
                        embedder.dims()
                    ),
                ));
            }
            let blob = embedding_to_blob(&vector);
            upd.execute(params![chunk_id, blob])?;
        }

        // Upsert rather than a plain UPDATE: a db_info row's presence is
        // guaranteed post-migration in practice, but reindex is exactly
        // the "something about this db's identity may be unusual" verb,
        // so it doesn't assume the row exists.
        conn.execute(
            "INSERT INTO db_info(key, value) VALUES ('embedder_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![crate::embed::EMBEDDER_ID],
        )?;
        conn.execute(
            "INSERT INTO db_info(key, value) VALUES ('embedder_dims', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![crate::embed::EMBEDDER_DIMS.to_string()],
        )?;

        Ok(rows.len())
    })();

    let chunks_reindexed = match result {
        Ok(n) => {
            conn.execute_batch("COMMIT;")?;
            n
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e);
        }
    };

    Ok(ReindexResponse {
        ok: true,
        op: "reindex",
        chunks_reindexed,
        backup: backup_path
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        previous_embedder: EmbedderInfo {
            id: previous_embedder_id,
            dims: previous_embedder_dims,
        },
        embedder: EmbedderInfo {
            id: crate::embed::EMBEDDER_ID.to_string(),
            dims: crate::embed::EMBEDDER_DIMS,
        },
    })
}
