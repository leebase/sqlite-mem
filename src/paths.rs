//! DB path resolution and the filesystem permission story
//! (architecture.md §20 "Paths").
//!
//! Precedence: `--db` flag > `SQLITE_MEM_DB` env > `./.sqlite-mem/memory.db`.
//! The default path's parent directory is created (`0700`) if missing; any
//! explicitly supplied path's parent must already exist. DB files are
//! created `0600`. Unix-only permission enforcement for S2 (Windows ACL
//! story is a later-sprint concern; the resolution logic itself is
//! platform-independent).

use crate::error::AppError;
use std::path::{Path, PathBuf};

pub const DEFAULT_DB_DIR: &str = ".sqlite-mem";
pub const DEFAULT_DB_FILE: &str = "memory.db";

/// Resolves the database path per the precedence above. Ensures the
/// default directory exists (creating it `0700`) when no explicit path was
/// given; verifies an explicit path's parent already exists otherwise.
pub fn resolve_db_path(cli_db: Option<&str>) -> Result<PathBuf, AppError> {
    if let Some(p) = cli_db {
        let path = PathBuf::from(p);
        ensure_parent_exists(&path)?;
        return Ok(path);
    }
    if let Ok(env_path) = std::env::var("SQLITE_MEM_DB") {
        if !env_path.is_empty() {
            let path = PathBuf::from(env_path);
            ensure_parent_exists(&path)?;
            return Ok(path);
        }
    }

    let dir = PathBuf::from(DEFAULT_DB_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| {
            AppError::database(
                "db_path_unavailable",
                format!("failed to create default db dir {}: {e}", dir.display()),
            )
        })?;
        set_dir_mode_0700(&dir);
    }
    Ok(dir.join(DEFAULT_DB_FILE))
}

fn ensure_parent_exists(path: &Path) -> Result<(), AppError> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            if !parent.exists() {
                return Err(AppError::database(
                    "db_path_unavailable",
                    format!(
                        "parent directory does not exist: {} (create it first, or omit --db to use the default path)",
                        parent.display()
                    ),
                ));
            }
            Ok(())
        }
        _ => Ok(()), // relative path with no directory component (cwd)
    }
}

#[cfg(unix)]
fn set_dir_mode_0700(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(dir) {
        let mut perms = meta.permissions();
        perms.set_mode(0o700);
        let _ = std::fs::set_permissions(dir, perms);
    }
}

#[cfg(not(unix))]
fn set_dir_mode_0700(_dir: &Path) {}

/// Restricts a just-created database file to `0600`.
#[cfg(unix)]
pub fn set_file_mode_0600(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
pub fn set_file_mode_0600(_path: &Path) {}

/// The path `info` reports: symlinks are followed for opening the DB (the
/// user's own file, the user's own choice) but `info` reports the resolved
/// (canonicalized) path per architecture.md §20.
pub fn resolved_path_for_display(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
