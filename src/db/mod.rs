//! Database open, pragmas, and the migration runner
//! (architecture.md §6, §10, §19).

mod migrations;

use crate::error::AppError;
use crate::paths;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Highest schema version this binary understands. A database whose
/// `PRAGMA user_version` exceeds this is refused (exit 6) rather than
/// silently opened with unknown tables/columns.
pub const SCHEMA_VERSION: i64 = migrations::MIGRATIONS[migrations::MIGRATIONS.len() - 1].0;

/// This binary's own version, recorded in `db_info.created_by_version`.
pub const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub struct Db {
    pub conn: Connection,
    pub path: PathBuf,
}

impl Db {
    /// Opens (creating if necessary) the database at `path`, applying
    /// pragmas and running any pending forward migrations.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let existed_before = path.exists();

        // Read the current schema version with a throwaway connection
        // *before* touching journal mode, so a pre-migration backup (taken
        // below, as a plain file copy) captures the database in its
        // simplest on-disk form -- no -wal/-shm siblings to reconcile.
        let current_version = {
            let probe = Connection::open(path)?;
            let v: i64 = probe.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            v
        };

        if current_version > SCHEMA_VERSION {
            return Err(AppError::version_mismatch(
                "schema_newer_than_binary",
                format!(
                    "database schema version {current_version} is newer than this binary supports (max {SCHEMA_VERSION})"
                ),
            )
            .with_hint("upgrade sqlite-mem to a version that supports this database"));
        }

        if current_version < SCHEMA_VERSION && existed_before {
            backup_file(path)?;
        }

        let conn = Connection::open(path)?;
        apply_runtime_pragmas(&conn)?;

        if !existed_before {
            paths::set_file_mode_0600(path);
        }

        if current_version < SCHEMA_VERSION {
            run_migrations(&conn, current_version, existed_before)?;
        }

        Ok(Db {
            conn,
            path: path.to_path_buf(),
        })
    }
}

fn apply_runtime_pragmas(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )?;
    Ok(())
}

/// Copies an existing database file to a timestamped `.bak` sibling.
/// Used both before any migration DDL runs (architecture.md §19) and by
/// `reindex` (S4, architecture.md §19: "reuse the migration backup
/// helper") before it rewrites every chunk's embedding. Returns the
/// backup's path.
pub(crate) fn backup_file(path: &Path) -> Result<PathBuf, AppError> {
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let bak_path = {
        let mut s = path.as_os_str().to_owned();
        s.push(format!(".bak.{ts}"));
        PathBuf::from(s)
    };
    std::fs::copy(path, &bak_path).map_err(|e| {
        AppError::database(
            "backup_failed",
            format!("failed to create backup {}: {e}", bak_path.display()),
        )
    })?;
    tracing::info!(backup = %bak_path.display(), "backup created");
    Ok(bak_path)
}

/// Compares this binary's embedder identity against the one recorded in
/// `db_info` at DB creation (architecture.md §19). `save` and
/// `ask --mode hybrid|semantic` call this and refuse (exit 6) on a
/// mismatch, hinting at `reindex`; `ask --mode lexical`, `info`, and
/// `forget` never call it -- content and metadata are never embedder-
/// dependent, so those verbs keep working on a DB stamped by a different
/// embedder (architecture.md §19: "save/ask --mode semantic|hybrid fail
/// with exit 6 and a hint; ask --mode lexical still works").
pub fn check_embedder_match(conn: &Connection) -> Result<(), AppError> {
    let db_embedder_id: Option<String> = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(db_embedder_id) = db_embedder_id else {
        // No recorded identity -- shouldn't happen post-migration (S2
        // always stamps it at db_info creation), but don't block on it.
        return Ok(());
    };
    if db_embedder_id != crate::embed::EMBEDDER_ID {
        return Err(AppError::version_mismatch(
            "embedder_mismatch",
            format!(
                "database was embedded with '{db_embedder_id}', this binary embeds with '{}'",
                crate::embed::EMBEDDER_ID
            ),
        )
        .with_hint(
            "run `sqlite-mem reindex` to re-embed this database with the current embedder",
        ));
    }
    Ok(())
}

fn run_migrations(
    conn: &Connection,
    from_version: i64,
    existed_before: bool,
) -> Result<(), AppError> {
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = (|| -> Result<(), AppError> {
        for (version, sql) in migrations::MIGRATIONS {
            if *version <= from_version {
                continue;
            }
            conn.execute_batch(sql)?;
            conn.pragma_update(None, "user_version", version)?;
        }
        if from_version == 0 {
            // First-ever creation of this file: stamp the embedder identity
            // and provenance (architecture.md §19). Never rewritten on a
            // later forward migration of an existing db -- embedder
            // identity changes only via `reindex` (S4).
            let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let mut stmt = conn.prepare("INSERT INTO db_info(key, value) VALUES (?1, ?2)")?;
            for (k, v) in [
                ("embedder_id", crate::embed::EMBEDDER_ID.to_string()),
                ("embedder_dims", crate::embed::EMBEDDER_DIMS.to_string()),
                ("created_by_version", BINARY_VERSION.to_string()),
                ("db_created_at", now),
            ] {
                stmt.execute(rusqlite::params![k, v])?;
            }
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT;")?;
            let action = if existed_before {
                "upgraded"
            } else {
                "created"
            };
            tracing::info!(schema_version = SCHEMA_VERSION, action, "database migrated");
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fresh_db_creates_schema_and_db_info() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        let db = Db::open(&path).unwrap();
        let v: i64 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let embedder_id: String = db
            .conn
            .query_row(
                "SELECT value FROM db_info WHERE key = 'embedder_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(embedder_id, crate::embed::EMBEDDER_ID);
    }

    #[test]
    fn reopening_a_current_db_is_a_noop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        Db::open(&path).unwrap();
        let db2 = Db::open(&path).unwrap();
        let v: i64 = db2
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 99)
                .unwrap();
        }
        let err = Db::open(&path).unwrap_err();
        assert_eq!(err.exit, crate::error::ExitCode::VersionMismatch);
        assert_eq!(err.code, "schema_newer_than_binary");
    }

    #[test]
    fn migrating_a_preexisting_file_creates_a_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        // A pre-existing file at user_version 0 (e.g. a bare empty file, or
        // one created by another tool) forces the migration path, which
        // must back it up before applying DDL.
        std::fs::write(&path, b"").unwrap();
        Db::open(&path).unwrap();

        let bak_count = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .count();
        assert_eq!(bak_count, 1, "expected exactly one .bak file");
    }

    #[test]
    fn check_embedder_match_passes_for_a_freshly_created_db() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("memory.db")).unwrap();
        assert!(check_embedder_match(&db.conn).is_ok());
    }

    #[test]
    fn check_embedder_match_fails_when_db_info_names_a_different_embedder() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("memory.db")).unwrap();
        db.conn
            .execute(
                "UPDATE db_info SET value = 'some-other-embedder' WHERE key = 'embedder_id'",
                [],
            )
            .unwrap();
        let err = check_embedder_match(&db.conn).unwrap_err();
        assert_eq!(err.exit, crate::error::ExitCode::VersionMismatch);
        assert_eq!(err.code, "embedder_mismatch");
        assert!(err.hint.unwrap().contains("reindex"));
    }

    #[test]
    fn backup_file_returns_a_path_containing_bak_and_a_timestamp() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        std::fs::write(&path, b"pretend db bytes").unwrap();
        let bak = backup_file(&path).unwrap();
        assert!(bak.exists());
        assert!(bak.to_string_lossy().contains(".bak."));
    }

    #[test]
    fn brand_new_default_path_creates_no_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.db");
        Db::open(&path).unwrap();
        let bak_count = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .count();
        assert_eq!(bak_count, 0);
    }
}
