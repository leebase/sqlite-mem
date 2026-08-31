//! Embedder-mismatch refusal matrix (architecture.md §19, project-plan.md
//! S4): `save` and `ask --mode hybrid|semantic` fail exit 6 with a hint
//! naming `reindex`; `ask --mode lexical`, `info`, and `forget` still work.

mod common;

use common::{bin_in, parse_single_json};
use predicates::prelude::*;
use rusqlite::Connection;
use tempfile::tempdir;

/// Builds a db with one active memory, then stamps `db_info.embedder_id`
/// with a value that can never match `crate::embed::EMBEDDER_ID`.
fn mismatched_db(dir: &std::path::Path) -> (std::path::PathBuf, String) {
    let db_path = dir.join("m.db");
    let out = bin_in(dir)
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "content saved before the embedder was swapped out",
        ])
        .assert()
        .success();
    let id = parse_single_json(&out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE db_info SET value = 'some-other-embedder-id' WHERE key = 'embedder_id'",
        [],
    )
    .unwrap();
    (db_path, id)
}

#[test]
fn save_fails_exit_6_with_a_reindex_hint_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, _id) = mismatched_db(dir.path());

    let out = bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "this save should be refused",
        ])
        .assert()
        .code(6);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "embedder_mismatch");
    assert!(v["error"]["hint"].as_str().unwrap().contains("reindex"));
}

#[test]
fn ask_hybrid_fails_exit_6_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, _id) = mismatched_db(dir.path());

    bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "hybrid",
            "--query",
            "content",
        ])
        .assert()
        .code(6)
        .stdout(predicate::str::contains("embedder_mismatch"));
}

#[test]
fn ask_semantic_fails_exit_6_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, _id) = mismatched_db(dir.path());

    bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "semantic",
            "--query",
            "content",
        ])
        .assert()
        .code(6)
        .stdout(predicate::str::contains("embedder_mismatch"));
}

#[test]
fn ask_lexical_still_works_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, id) = mismatched_db(dir.path());

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "lexical",
            "--query",
            "content",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["results"][0]["id"], id);
}

#[test]
fn info_still_works_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, _id) = mismatched_db(dir.path());

    let out = bin_in(dir.path())
        .args(["info", "--db", db_path.to_str().unwrap()])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["embedder"]["id"], "some-other-embedder-id");
}

#[test]
fn forget_still_works_on_mismatch() {
    let dir = tempdir().unwrap();
    let (db_path, id) = mismatched_db(dir.path());

    let out = bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["results"][0]["status"], "forgotten");
}
