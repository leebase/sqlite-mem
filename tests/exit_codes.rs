//! Exit-code table coverage for the verbs S2 implements (project-plan.md
//! S2: "exit codes 0/2/3/5/6"). Codes 4 (not found) and 7 (integrity) have
//! no reachable path yet -- `forget`/`ask`/`info --verify` are later-sprint
//! scope (non-goals, project-plan.md S2) -- so they are not asserted here.

mod common;

use common::bin_in;
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn code_0_on_success() {
    let dir = tempdir().unwrap();
    bin_in(dir.path()).args(["info"]).assert().code(0);
}

#[test]
fn code_2_on_usage_error() {
    let dir = tempdir().unwrap();
    bin_in(dir.path()).args(["save"]).assert().code(2);
}

#[test]
fn code_3_on_validation_error() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", ""])
        .assert()
        .code(3);
}

#[test]
fn code_5_on_database_error() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("missing-parent").join("memory.db");
    bin_in(dir.path())
        .args(["info", "--db", bad.to_str().unwrap()])
        .assert()
        .code(5);
}

#[test]
fn code_6_on_version_mismatch() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("memory.db");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 42i64).unwrap();
    }
    bin_in(dir.path())
        .args(["save", "--content", "x", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(6);
}
