//! `info` (basic) contract tests (project-plan.md S2), extended in S4 with
//! `info --verify` (architecture.md §18): integrity_check, FTS-vs-chunks
//! consistency, embedding-dims audit, content_hash spot-check, and that a
//! deliberately corrupted db is caught (exit 7).

mod common;

use common::{bin_in, parse_single_json};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn info_on_fresh_db_reports_zero_counts() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path()).args(["info"]).assert().success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "info");
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["embedder"]["id"], "granite-embedding-small-english-r2");
    assert_eq!(v["embedder"]["dims"], 384);
    assert_eq!(v["counts"]["active"], 0);
    assert_eq!(v["counts"]["superseded"], 0);
    assert_eq!(v["counts"]["forgotten"], 0);
    assert_eq!(v["chunks"], 0);
    assert!(v["db_size_bytes"].as_u64().unwrap() > 0);
    assert!(v["path"].as_str().unwrap().ends_with("memory.db"));
}

#[test]
fn info_reflects_saved_and_superseded_memories() {
    let dir = tempdir().unwrap();
    let saved = bin_in(dir.path())
        .args(["save", "--content", "first memory"])
        .assert()
        .success();
    let first_id = parse_single_json(&saved.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "second memory",
            "--supersedes",
            &first_id,
        ])
        .assert()
        .success();

    let out = bin_in(dir.path()).args(["info"]).assert().success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["counts"]["active"], 1);
    assert_eq!(v["counts"]["superseded"], 1);
    assert_eq!(v["chunks"], 2);
}

#[test]
fn verify_on_a_healthy_db_passes_every_check_exit_0() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a perfectly healthy memory",
            "--meta",
            "kind=decision",
        ])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(0);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert!(
        v.get("error").is_none(),
        "no error field on a passing verify"
    );
    assert_eq!(v["op"], "info");
    assert_eq!(v["verify"], true);
    assert_eq!(v["checks"]["integrity_check"]["pass"], true);
    assert_eq!(v["checks"]["fts_consistency"]["pass"], true);
    assert_eq!(v["checks"]["embedding_dims"]["pass"], true);
    assert_eq!(v["checks"]["content_hash"]["pass"], true);
}

#[test]
fn verify_on_empty_fresh_db_passes() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["info", "--verify"])
        .assert()
        .code(0);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
}

#[test]
fn verify_detects_a_flipped_content_byte_via_content_hash_check_exit_7() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let out = bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "content that will be tampered with directly via sql",
        ])
        .assert()
        .success();
    let id = parse_single_json(&out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE memories SET content = 'tampered content, hash no longer matches' WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    // architecture.md §18, amended: every non-zero exit pairs with
    // ok:false, uniformly -- a failed verify is no exception.
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "integrity_failed");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("content_hash"));
    assert!(!v["error"]["hint"].as_str().unwrap().is_empty());
    // The checks object is still fully present alongside the error.
    assert_eq!(v["checks"]["content_hash"]["pass"], false);
    assert!(v["checks"]["content_hash"]["detail"]
        .as_str()
        .unwrap()
        .contains(&id));
    // Corrupting a text column doesn't break SQLite's own b-tree
    // structure, so the unrelated checks stay green.
    assert_eq!(v["checks"]["integrity_check"]["pass"], true);
    assert_eq!(v["checks"]["embedding_dims"]["pass"], true);
}

#[test]
fn verify_detects_a_truncated_embedding_blob_via_dims_check_exit_7() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory whose embedding will be truncated",
        ])
        .assert()
        .success();

    {
        let conn = Connection::open(&db_path).unwrap();
        let chunk_id: String = conn
            .query_row("SELECT id FROM chunks LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "UPDATE chunks SET embedding = substr(embedding, 1, 4) WHERE id = ?1",
            [&chunk_id],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "integrity_failed");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("embedding_dims"));
    assert_eq!(v["checks"]["embedding_dims"]["pass"], false);
    // Content is untouched, so the hash check still passes.
    assert_eq!(v["checks"]["content_hash"]["pass"], true);
}
