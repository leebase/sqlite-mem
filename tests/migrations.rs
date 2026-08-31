//! Migration runner tests at the CLI boundary (project-plan.md S2):
//! `.bak` creation and newer-schema refusal (exit 6).
//!
//! (The same behavior is also unit-tested against `db::Db` directly in
//! `src/db/mod.rs`; these exercise it through the real binary.)

mod common;

use common::bin_in;
use predicates::prelude::*;
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn preexisting_file_at_default_path_is_backed_up_before_migration() {
    let dir = tempdir().unwrap();
    let sqlite_mem_dir = dir.path().join(".sqlite-mem");
    std::fs::create_dir_all(&sqlite_mem_dir).unwrap();
    let db_path = sqlite_mem_dir.join("memory.db");
    // A bare empty file at user_version 0 forces the migration path.
    std::fs::write(&db_path, b"").unwrap();

    bin_in(dir.path())
        .args(["save", "--content", "triggers a migration"])
        .assert()
        .success();

    let bak_count = std::fs::read_dir(&sqlite_mem_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
        .count();
    assert_eq!(
        bak_count, 1,
        "expected exactly one .bak file after migrating a pre-existing db"
    );
}

#[test]
fn newer_schema_than_binary_is_refused_with_exit_6() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("memory.db");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 999i64).unwrap();
    }

    bin_in(dir.path())
        .args(["info", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(6)
        .stdout(predicate::str::contains("schema_newer_than_binary"));
}

#[test]
fn brand_new_db_at_explicit_path_creates_no_backup() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("memory.db");

    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "fresh db, no backup expected",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let bak_count = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
        .count();
    assert_eq!(bak_count, 0);
}
