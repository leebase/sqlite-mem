//! `reindex` contract tests (project-plan.md S4, architecture.md §19):
//! re-embeds every chunk, works even when `db_info` names a different
//! (fake, old) embedder id, creates a `.bak` first, and updates
//! `db_info`'s embedder id/dims on success.

mod common;

use common::{bin_in, parse_single_json};
use rusqlite::Connection;
use tempfile::tempdir;

fn bak_count(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
        .count()
}

fn stamp_fake_embedder(db_path: &std::path::Path) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "UPDATE db_info SET value = 'fake-old-embedder-v0' WHERE key = 'embedder_id'",
        [],
    )
    .unwrap();
}

#[test]
fn reindex_re_embeds_every_chunk_and_creates_a_backup() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "first memory to reindex",
        ])
        .assert()
        .success();
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "second memory to reindex",
        ])
        .assert()
        .success();

    assert_eq!(bak_count(dir.path()), 0);

    let out = bin_in(dir.path())
        .args(["reindex", "--db", db_path.to_str().unwrap()])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "reindex");
    assert_eq!(v["chunks_reindexed"], 2);
    assert_eq!(v["embedder"]["id"], "granite-embedding-small-english-r2");
    assert_eq!(v["embedder"]["dims"], 384);
    assert!(!v["backup"].as_str().unwrap().is_empty());

    assert_eq!(
        bak_count(dir.path()),
        1,
        "reindex must create exactly one .bak"
    );
}

#[test]
fn reindex_recovers_a_db_stamped_with_a_fake_old_embedder_id() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "content saved under the real embedder",
        ])
        .assert()
        .success();

    stamp_fake_embedder(&db_path);

    // Confirm the mismatch actually blocks save/ask-hybrid first.
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "blocked by mismatch",
        ])
        .assert()
        .code(6);

    let out = bin_in(dir.path())
        .args(["reindex", "--db", db_path.to_str().unwrap()])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["previous_embedder"]["id"], "fake-old-embedder-v0");
    assert_eq!(v["embedder"]["id"], "granite-embedding-small-english-r2");

    // db_info is updated on success.
    let conn = Connection::open(&db_path).unwrap();
    let embedder_id: String = conn
        .query_row(
            "SELECT value FROM db_info WHERE key = 'embedder_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(embedder_id, "granite-embedding-small-english-r2");

    // Now save and hybrid ask both work again.
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "save works again after reindex",
        ])
        .assert()
        .success();
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
        .success();
}

#[test]
fn reindex_on_a_fresh_db_at_default_embedder_is_a_noop_success() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args(["info", "--db", db_path.to_str().unwrap()])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args(["reindex", "--db", db_path.to_str().unwrap()])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["chunks_reindexed"], 0);
}
