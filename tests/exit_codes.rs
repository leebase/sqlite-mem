//! Exit-code table coverage for the verbs implemented so far (project-plan.md
//! S2: "exit codes 0/2/3/5/6"; S3 extends this with `ask`'s own usage/
//! validation paths). Codes 4 (not found) and 7 (integrity) have no
//! reachable path yet -- `forget`/`info --verify` are later-sprint scope --
//! so they are not asserted here.

mod common;

use common::{bin_in, parse_single_json};
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

#[test]
fn ask_code_0_on_success_even_with_no_results() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["ask", "--query", "anything"])
        .assert()
        .code(0);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["results"], serde_json::json!([]));
}

#[test]
fn ask_code_2_when_query_and_stdin_both_missing() {
    let dir = tempdir().unwrap();
    bin_in(dir.path()).args(["ask"]).assert().code(2);
}

#[test]
fn ask_code_2_when_query_and_stdin_both_given() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["ask", "--query", "x", "--stdin"])
        .write_stdin("y")
        .assert()
        .code(2);
}

#[test]
fn ask_code_2_on_unknown_mode_value() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["ask", "--query", "x", "--mode", "bogus"])
        .assert()
        .code(2);
}

#[test]
fn ask_code_2_on_k_out_of_range() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["ask", "--query", "x", "--k", "0"])
        .assert()
        .code(2);
    bin_in(dir.path())
        .args(["ask", "--query", "x", "--k", "51"])
        .assert()
        .code(2);
}

#[test]
fn ask_code_2_on_malformed_where_term() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["ask", "--query", "x", "--where", "noequalssign"])
        .assert()
        .code(2);
}

#[test]
fn ask_code_3_on_empty_query() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["ask", "--query", "   "])
        .assert()
        .code(3);
}

#[test]
fn ask_code_3_on_oversized_query() {
    let dir = tempdir().unwrap();
    let big = "a".repeat(8193);
    bin_in(dir.path())
        .args(["ask", "--stdin"])
        .write_stdin(big)
        .assert()
        .code(3);
}
