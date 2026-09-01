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

// -- S6 audit F1: the fts_consistency check was unfalsifiable (the bare
// `INSERT INTO chunks_fts(chunks_fts) VALUES('integrity-check')` form always
// succeeds on an external-content table). These three desync modes are the
// ones the auditor used to prove the rank-1 form actually catches real
// corruption; each must fail `fts_consistency` specifically and exit 7.

#[test]
fn verify_detects_fts_index_wiped_via_delete_command_exit_7() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory whose fts index entries will be wiped",
        ])
        .assert()
        .success();

    {
        let conn = Connection::open(&db_path).unwrap();
        // FTS5's 'delete' special command removes the index entries for the
        // given rowid/text without touching the `chunks` content table --
        // a clean desync between the shadow index and its content.
        conn.execute_batch(
            "INSERT INTO chunks_fts(chunks_fts, rowid, text)
             SELECT 'delete', rowid, text FROM chunks;",
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["fts_consistency"]["pass"], false);
}

#[test]
fn verify_detects_fts_desync_via_dropped_sync_trigger_exit_7() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let out = bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory whose sync trigger will be dropped before an edit",
        ])
        .assert()
        .success();
    let id = parse_single_json(&out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    {
        let conn = Connection::open(&db_path).unwrap();
        // Drop the AFTER UPDATE trigger that keeps chunks_fts in sync, then
        // edit chunk text directly via SQL -- the content table and the fts
        // shadow index now disagree about what that chunk's text is.
        conn.execute_batch("DROP TRIGGER IF EXISTS chunks_au;")
            .unwrap();
        conn.execute(
            "UPDATE chunks SET text = 'this text no longer matches the fts index' \
             WHERE memory_id = ?1",
            [&id],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["fts_consistency"]["pass"], false);
}

#[test]
fn verify_detects_a_single_deleted_fts_index_row_exit_7() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory that will lose exactly one fts index row",
        ])
        .assert()
        .success();

    {
        let conn = Connection::open(&db_path).unwrap();
        let rowid: i64 = conn
            .query_row("SELECT rowid FROM chunks LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let text: String = conn
            .query_row("SELECT text FROM chunks WHERE rowid = ?1", [rowid], |r| {
                r.get(0)
            })
            .unwrap();
        // Delete just this one row's fts index entry (content table
        // untouched) -- the previous COUNT(*)-based check would have caught
        // this (chunks_fts's count reads through to `chunks`, so it never
        // actually changes either way); only the real integrity-check
        // command can.
        conn.execute(
            "INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', ?1, ?2)",
            rusqlite::params![rowid, text],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["fts_consistency"]["pass"], false);
}

// -- S6 audit F11: the content_hash spot-check sampled only the 100 OLDEST
// memories (`ORDER BY id LIMIT 100`), so tampering with a recently-saved
// memory on a db past ~100 memories was never checked. Stride sampling
// anchored from the newest row must always catch it.

#[test]
fn verify_catches_tampering_of_the_newest_memory_past_the_old_sample_cutoff() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");

    // 150 memories: well past the old hard LIMIT 100 cutoff, and not an
    // exact multiple of any "nice" stride, so this doesn't accidentally
    // pass by coincidence.
    for i in 0..150 {
        bin_in(dir.path())
            .args([
                "save",
                "--db",
                db_path.to_str().unwrap(),
                "--content",
                &format!("memory number {i}"),
            ])
            .assert()
            .success();
    }

    // Verify passes before tampering.
    bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(0);

    let newest_id: String = {
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT id FROM memories ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE memories SET content = 'tampered newest memory' WHERE id = ?1",
            [&newest_id],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["content_hash"]["pass"], false);
    assert!(v["checks"]["content_hash"]["detail"]
        .as_str()
        .unwrap()
        .contains(&newest_id));
}

// -- S6 audit info item (b): a non-numeric/missing db_info.embedder_dims
// used to fall back silently to 384 even in --verify's own dims-audit.

#[test]
fn verify_fails_dims_audit_when_embedder_dims_is_unparseable() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory in a db whose embedder_dims record will be corrupted",
        ])
        .assert()
        .success();

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE db_info SET value = 'not-a-number' WHERE key = 'embedder_dims'",
            [],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["embedding_dims"]["pass"], false);
}

#[test]
fn verify_fails_dims_audit_when_embedder_dims_row_is_missing() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "a memory in a db whose embedder_dims record will be deleted",
        ])
        .assert()
        .success();

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute("DELETE FROM db_info WHERE key = 'embedder_dims'", [])
            .unwrap();
    }

    let out = bin_in(dir.path())
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .assert()
        .code(7);
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["checks"]["embedding_dims"]["pass"], false);
}
