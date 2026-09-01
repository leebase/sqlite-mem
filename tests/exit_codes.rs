//! Exit-code table coverage (project-plan.md S2: "exit codes 0/2/3/5/6"; S3
//! extends this with `ask`'s own usage/validation paths; S4 closes the
//! table with 4 (not found, via `forget`) and 7 (integrity, via
//! `info --verify`) -- see `tests/forget_contract.rs` and
//! `tests/info_contract.rs` for the fuller state-machine/corruption
//! coverage of each.

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

// S6 audit F5: `ask --stdin` read unboundedly before checking the 8 KiB
// query cap; bound it to cap+1 bytes and prove a large pipe is rejected
// quickly, plus an exact cap+2 boundary case.
#[test]
fn ask_code_3_on_oversized_stdin_query_at_cap_plus_two_bytes() {
    let dir = tempdir().unwrap();
    let big = "a".repeat(8192 + 2);
    let out = bin_in(dir.path())
        .args(["ask", "--stdin"])
        .write_stdin(big)
        .assert()
        .code(3);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["error"]["code"], "input_too_large");
}

#[test]
fn ask_oversized_stdin_query_is_rejected_quickly() {
    let dir = tempdir().unwrap();
    let big = "a".repeat(64 * 1024 * 1024);
    let started = std::time::Instant::now();
    bin_in(dir.path())
        .args(["ask", "--stdin"])
        .write_stdin(big)
        .assert()
        .code(3);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "a bounded stdin read should reject an oversized query promptly, took {:?}",
        started.elapsed()
    );
}

// S6 audit F10: invalid UTF-8 on stdin used to be a usage error (exit 2);
// per architecture.md §17 malformed *content* is a validation failure
// (exit 3), not a malformed invocation.
#[test]
fn save_code_3_on_invalid_utf8_stdin() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--stdin"])
        .write_stdin(vec![0xFFu8, 0xFE, 0xFD, b'x'])
        .assert()
        .code(3);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "invalid_utf8");
}

#[test]
fn ask_code_3_on_invalid_utf8_stdin() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["ask", "--stdin"])
        .write_stdin(vec![0xFFu8, 0xFE, 0xFD, b'x'])
        .assert()
        .code(3);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "invalid_utf8");
}

#[test]
fn code_4_on_forget_unknown_id() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["forget", "01ARZ3NDEKTSV4RRFFQ69G5FAV"])
        .assert()
        .code(4);
}

// -- S6 audit F2: `--db ""` used to fall through to SQLite's anonymous temp
// database (every write reports ok:true, then is silently discarded on
// exit) instead of erroring. All three verbs share `paths::resolve_db_path`,
// so one representative test each is enough to prove the guard is wired
// into every call site.

#[test]
fn save_code_2_on_empty_db_path() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "x", "--db", ""])
        .assert()
        .code(2);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "empty_db_path");
}

#[test]
fn save_code_2_on_whitespace_only_db_path() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "x", "--db", "   "])
        .assert()
        .code(2);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["error"]["code"], "empty_db_path");
}

#[test]
fn ask_code_2_on_empty_db_path() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["ask", "--query", "x", "--db", ""])
        .assert()
        .code(2);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["error"]["code"], "empty_db_path");
}

#[test]
fn info_code_2_on_empty_db_path() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["info", "--db", ""])
        .assert()
        .code(2);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["error"]["code"], "empty_db_path");
}

#[test]
fn code_7_on_verify_failure() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let out = bin_in(dir.path())
        .args(["save", "--db", db_path.to_str().unwrap(), "--content", "x"])
        .assert()
        .success();
    let id = parse_single_json(&out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE memories SET content = 'tampered' WHERE id = ?1",
        [&id],
    )
    .unwrap();
    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    // architecture.md §18, amended: every non-zero exit pairs with
    // ok:false, uniformly -- including info --verify's own failure path.
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "integrity_failed");
    assert_eq!(v["checks"]["content_hash"]["pass"], false);
}
