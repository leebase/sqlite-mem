//! `save` verb (architecture.md §11).

use crate::chunk;
use crate::db::Db;
use crate::embed::Embedder;
use crate::error::AppError;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use ulid::Ulid;

pub const MAX_CONTENT_BYTES: usize = 1_048_576; // 1 MiB
pub const MAX_META_PAIRS: usize = 64;
pub const MAX_META_KEY_BYTES: usize = 128;
pub const MAX_META_VALUE_BYTES: usize = 4096; // 4 KiB

/// Already-parsed and validated `save` input. `cli.rs` owns turning raw
/// clap output into this; nothing here reads `std::env::args`.
pub struct SaveInput {
    pub content: String,
    pub source: Option<String>,
    pub meta: Vec<(String, String)>,
    pub supersedes: Vec<String>,
    pub if_new: bool,
}

#[derive(Serialize)]
struct EmbedderInfo {
    id: String,
    dims: usize,
}

#[derive(Serialize)]
pub struct SaveResponse {
    ok: bool,
    op: &'static str,
    id: String,
    deduplicated: bool,
    chunks: usize,
    content_hash: String,
    created_at: String,
    superseded: Vec<String>,
    embedder: EmbedderInfo,
}

/// Validates raw `--meta KEY=VALUE` strings into `(key, value)` pairs.
/// Split out so it is unit-testable without touching the filesystem.
pub fn parse_and_validate_meta(raw: &[String]) -> Result<Vec<(String, String)>, AppError> {
    if raw.len() > MAX_META_PAIRS {
        return Err(AppError::validation(
            "too_many_meta_pairs",
            format!(
                "at most {MAX_META_PAIRS} --meta pairs are allowed, got {}",
                raw.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let (k, v) = entry.split_once('=').ok_or_else(|| {
            AppError::validation(
                "invalid_meta_pair",
                format!("--meta expects KEY=VALUE, got '{entry}'"),
            )
        })?;
        validate_meta_key(k)?;
        validate_meta_value(v)?;
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

fn validate_meta_key(key: &str) -> Result<(), AppError> {
    if key.is_empty() {
        return Err(AppError::validation(
            "invalid_meta_key",
            "metadata key must not be empty",
        ));
    }
    if key.len() > MAX_META_KEY_BYTES {
        return Err(AppError::validation(
            "invalid_meta_key",
            format!("metadata key '{key}' exceeds {MAX_META_KEY_BYTES} bytes"),
        ));
    }
    let ok = key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-');
    if !ok {
        return Err(AppError::validation(
            "invalid_meta_key",
            format!("metadata key '{key}' must match [A-Za-z0-9_.-]+"),
        ));
    }
    Ok(())
}

fn validate_meta_value(value: &str) -> Result<(), AppError> {
    if value.len() > MAX_META_VALUE_BYTES {
        return Err(AppError::validation(
            "invalid_meta_value",
            format!("metadata value exceeds {MAX_META_VALUE_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn validate_content(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "empty_content",
            "content is empty after trimming",
        ));
    }
    if trimmed.len() > MAX_CONTENT_BYTES {
        return Err(AppError::validation(
            "input_too_large",
            format!(
                "content is {} bytes, exceeding the {MAX_CONTENT_BYTES}-byte cap",
                trimmed.len()
            ),
        ));
    }
    Ok(trimmed.to_string())
}

/// `pub(crate)` so `info --verify`'s content_hash spot-check (S4) recomputes
/// the exact same hash rather than duplicating the sha256 call site.
pub(crate) fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Marks each `active` id in `targets` as `superseded`, pointing
/// `superseded_by` at `by_id`. Shared by both the insert path and the
/// dedup path (architecture.md §11.2, amended post-S2-review: a dedup hit
/// must not silently drop the caller's retire-intent -- a retried
/// `save --supersedes` has to retire old truth exactly as the first
/// attempt would have). Self-supersession (`target == by_id`) is ignored:
/// a memory never supersedes itself. Targets that are missing or already
/// non-active are silently skipped (idempotent: a second identical call
/// changes nothing further and reports an empty `superseded` list).
fn apply_supersedes(
    conn: &Connection,
    by_id: &str,
    targets: &[String],
) -> Result<Vec<String>, AppError> {
    let mut superseded_ids = Vec::new();
    let mut supersede_stmt = conn.prepare(
        "UPDATE memories SET status = 'superseded', superseded_by = ?1 WHERE id = ?2 AND status = 'active'",
    )?;
    for target in targets {
        if target == by_id {
            continue;
        }
        let affected = supersede_stmt.execute(params![by_id, target])?;
        if affected == 1 {
            superseded_ids.push(target.clone());
        }
    }
    Ok(superseded_ids)
}

pub fn run(db_path: &std::path::Path, input: SaveInput) -> Result<SaveResponse, AppError> {
    let content = validate_content(&input.content)?;
    let content_hash = sha256_hex(&content);

    let db = Db::open(db_path)?;
    let conn = &db.conn;

    // Embedder-mismatch refusal (architecture.md §19, S4): `save` always
    // embeds (even a dedup hit reaches supersession logic that doesn't
    // need re-embedding, but the contract draws no such exception), so
    // this is checked unconditionally right after opening the db.
    crate::db::check_embedder_match(conn)?;

    // Dedup (§11.2): an active memory with the identical content_hash is
    // returned as-is, deduplicated:true, no insert -- but any
    // `--supersedes` targets are still retired, pointing at the *existing*
    // memory (see `apply_supersedes`).
    let existing: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT m.id, m.created_at, (SELECT COUNT(*) FROM chunks c WHERE c.memory_id = m.id)
             FROM memories m WHERE m.content_hash = ?1 AND m.status = 'active' LIMIT 1",
            params![content_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional_or_db_err()?;

    if let Some((existing_id, existing_created_at, existing_chunks)) = existing {
        if input.if_new {
            return Err(AppError::validation(
                "not_new",
                format!("an active memory with identical content already exists (id {existing_id}) and --if-new was set"),
            ));
        }
        conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result = apply_supersedes(conn, &existing_id, &input.supersedes);
        let superseded = match result {
            Ok(superseded) => {
                conn.execute_batch("COMMIT;")?;
                superseded
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
        };

        return Ok(SaveResponse {
            ok: true,
            op: "save",
            id: existing_id,
            deduplicated: true,
            chunks: existing_chunks as usize,
            content_hash: format!("sha256:{content_hash}"),
            created_at: existing_created_at,
            superseded,
            embedder: EmbedderInfo {
                id: crate::embed::EMBEDDER_ID.to_string(),
                dims: crate::embed::EMBEDDER_DIMS,
            },
        });
    }

    let embedder = Embedder::load()?;

    let chunks = chunk::chunk(&content);
    let mut chunk_rows: Vec<(String, i64, String, Vec<u8>)> = Vec::with_capacity(chunks.len());
    let new_id = Ulid::new().to_string();
    for (idx, c) in chunks.iter().enumerate() {
        let vector = embedder.embed(&c.text)?;
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
        chunk_rows.push((format!("{new_id}:{idx}"), idx as i64, c.text.clone(), blob));
    }

    let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = (|| -> Result<Vec<String>, AppError> {
        conn.execute(
            "INSERT INTO memories (id, content, content_hash, source, created_at, status, superseded_by, forgotten_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', NULL, NULL)",
            params![new_id, content, content_hash, input.source, created_at],
        )?;

        {
            let mut meta_stmt = conn
                .prepare("INSERT INTO memory_meta (memory_id, key, value) VALUES (?1, ?2, ?3)")?;
            for (k, v) in &input.meta {
                meta_stmt.execute(params![new_id, k, v])?;
            }
        }

        {
            let mut chunk_stmt = conn.prepare(
                "INSERT INTO chunks (id, memory_id, idx, text, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (chunk_id, idx, text, blob) in &chunk_rows {
                chunk_stmt.execute(params![chunk_id, new_id, idx, text, blob])?;
            }
        }

        apply_supersedes(conn, &new_id, &input.supersedes)
    })();

    let superseded = match result {
        Ok(superseded) => {
            conn.execute_batch("COMMIT;")?;
            superseded
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e);
        }
    };

    Ok(SaveResponse {
        ok: true,
        op: "save",
        id: new_id,
        deduplicated: false,
        chunks: chunk_rows.len(),
        content_hash: format!("sha256:{content_hash}"),
        created_at,
        superseded,
        embedder: EmbedderInfo {
            id: crate::embed::EMBEDDER_ID.to_string(),
            dims: crate::embed::EMBEDDER_DIMS,
        },
    })
}

/// Small helper trait so the dedup lookup reads `.optional_or_db_err()`
/// instead of a `match ... QueryReturnedNoRows => None` block inline.
trait OptionalRow<T> {
    fn optional_or_db_err(self) -> Result<Option<T>, AppError>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional_or_db_err(self) -> Result<Option<T>, AppError> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_trimmed_and_accepted() {
        let v = validate_content("  hello  ").unwrap();
        assert_eq!(v, "hello");
    }

    #[test]
    fn empty_content_after_trim_is_rejected() {
        let err = validate_content("   \n\t  ").unwrap_err();
        assert_eq!(err.code, "empty_content");
        assert_eq!(err.exit, crate::error::ExitCode::Validation);
    }

    #[test]
    fn oversized_content_is_rejected() {
        let big = "a".repeat(MAX_CONTENT_BYTES + 1);
        let err = validate_content(&big).unwrap_err();
        assert_eq!(err.code, "input_too_large");
    }

    #[test]
    fn content_at_exact_cap_is_accepted() {
        let exact = "a".repeat(MAX_CONTENT_BYTES);
        assert!(validate_content(&exact).is_ok());
    }

    #[test]
    fn meta_key_accepts_allowed_charset() {
        assert!(validate_meta_key("kind").is_ok());
        assert!(validate_meta_key("kind.sub-key_1").is_ok());
    }

    #[test]
    fn meta_key_rejects_disallowed_chars() {
        for bad in ["has space", "colon:here", "slash/here", "quote\"here"] {
            let err = validate_meta_key(bad).unwrap_err();
            assert_eq!(err.code, "invalid_meta_key");
        }
    }

    #[test]
    fn meta_key_rejects_empty() {
        assert_eq!(validate_meta_key("").unwrap_err().code, "invalid_meta_key");
    }

    #[test]
    fn meta_key_rejects_oversized() {
        let big = "a".repeat(MAX_META_KEY_BYTES + 1);
        assert_eq!(
            validate_meta_key(&big).unwrap_err().code,
            "invalid_meta_key"
        );
    }

    #[test]
    fn meta_key_accepts_exact_cap() {
        let exact = "a".repeat(MAX_META_KEY_BYTES);
        assert!(validate_meta_key(&exact).is_ok());
    }

    #[test]
    fn meta_value_rejects_oversized() {
        let big = "a".repeat(MAX_META_VALUE_BYTES + 1);
        assert_eq!(
            validate_meta_value(&big).unwrap_err().code,
            "invalid_meta_value"
        );
    }

    #[test]
    fn meta_value_accepts_exact_cap() {
        let exact = "a".repeat(MAX_META_VALUE_BYTES);
        assert!(validate_meta_value(&exact).is_ok());
    }

    #[test]
    fn parse_meta_splits_on_first_equals_only() {
        let out = parse_and_validate_meta(&["k=v=with=equals".to_string()]).unwrap();
        assert_eq!(out, vec![("k".to_string(), "v=with=equals".to_string())]);
    }

    #[test]
    fn parse_meta_rejects_missing_equals() {
        let err = parse_and_validate_meta(&["noequals".to_string()]).unwrap_err();
        assert_eq!(err.code, "invalid_meta_pair");
    }

    #[test]
    fn parse_meta_rejects_too_many_pairs() {
        let raw: Vec<String> = (0..MAX_META_PAIRS + 1).map(|i| format!("k{i}=v")).collect();
        let err = parse_and_validate_meta(&raw).unwrap_err();
        assert_eq!(err.code, "too_many_meta_pairs");
    }

    #[test]
    fn parse_meta_accepts_exact_pair_cap() {
        let raw: Vec<String> = (0..MAX_META_PAIRS).map(|i| format!("k{i}=v")).collect();
        assert!(parse_and_validate_meta(&raw).is_ok());
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("") -- standard test vector.
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_matches_known_vector_hello() {
        assert_eq!(
            sha256_hex("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn embedding_to_blob_round_trips_little_endian() {
        let v = vec![1.0f32, -2.5, 0.0];
        let blob = embedding_to_blob(&v);
        assert_eq!(blob.len(), 12);
        let mut restored = Vec::new();
        for chunk in blob.as_chunks::<4>().0 {
            restored.push(f32::from_le_bytes(*chunk));
        }
        assert_eq!(restored, v);
    }
}
