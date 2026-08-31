//! `forget` / `--purge` / `--restore` contract tests (project-plan.md S4,
//! architecture.md §15): state machine transitions, cascades on purge,
//! all-or-nothing multi-id behavior.

mod common;

use common::{bin_in, parse_single_json};
use rusqlite::Connection;
use tempfile::tempdir;

fn save_id(dir: &std::path::Path, db_path: &std::path::Path, content: &str) -> String {
    let out = bin_in(dir)
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            content,
        ])
        .assert()
        .success();
    parse_single_json(&out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn memory_row(db_path: &std::path::Path, id: &str) -> (String, Option<String>, Option<String>) {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT status, superseded_by, forgotten_at FROM memories WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap()
}

#[test]
fn forget_marks_status_forgotten_and_stamps_forgotten_at() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "a memory to forget");

    let out = bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "forget");
    assert_eq!(v["mode"], "forget");
    assert_eq!(v["destructive"], false);
    assert_eq!(v["count"], 1);
    assert_eq!(v["results"][0]["id"], id);
    assert_eq!(v["results"][0]["status"], "forgotten");
    assert_eq!(v["results"][0]["changed"], true);
    assert!(v["results"][0]["forgotten_at"]
        .as_str()
        .unwrap()
        .ends_with('Z'));

    let (status, _, forgotten_at) = memory_row(&db_path, &id);
    assert_eq!(status, "forgotten");
    assert!(forgotten_at.is_some());
}

#[test]
fn forget_is_idempotent_and_keeps_the_original_forgotten_at() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "forget me twice");

    bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();
    let (_, _, first_forgotten_at) = memory_row(&db_path, &id);

    let out = bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"][0]["changed"], false);
    assert_eq!(
        v["results"][0]["forgotten_at"].as_str().map(String::from),
        first_forgotten_at
    );

    let (_, _, second_forgotten_at) = memory_row(&db_path, &id);
    assert_eq!(first_forgotten_at, second_forgotten_at);
}

#[test]
fn ask_excludes_forgotten_memories_by_default_but_include_flag_finds_them() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "unique searchable phrase zephyrtoken");

    bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "lexical",
            "--query",
            "zephyrtoken",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"], serde_json::json!([]));

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "lexical",
            "--include-forgotten",
            "--query",
            "zephyrtoken",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"][0]["id"], id);
}

#[test]
fn restore_returns_a_plain_forgotten_memory_to_active() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "restore me");

    bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--restore",
            &id,
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["mode"], "restore");
    assert_eq!(v["results"][0]["status"], "active");
    assert_eq!(v["results"][0]["changed"], true);
    assert!(v["results"][0]["forgotten_at"].is_null());

    let (status, _, forgotten_at) = memory_row(&db_path, &id);
    assert_eq!(status, "active");
    assert!(forgotten_at.is_none());
}

#[test]
fn restore_of_a_forgotten_superseded_memory_returns_to_superseded_not_active() {
    // architecture.md §15: "a memory that is superseded stays superseded --
    // restore only undoes forget". A memory that was superseded and then
    // separately forgotten must come back as `superseded`, not `active`.
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let old_id = save_id(
        dir.path(),
        &db_path,
        "old truth, later superseded and forgotten",
    );
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "new truth",
            "--supersedes",
            &old_id,
        ])
        .assert()
        .success();

    let (status, superseded_by, _) = memory_row(&db_path, &old_id);
    assert_eq!(status, "superseded");
    assert!(superseded_by.is_some());

    bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &old_id])
        .assert()
        .success();
    let (status, _, _) = memory_row(&db_path, &old_id);
    assert_eq!(status, "forgotten");

    let out = bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--restore",
            &old_id,
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"][0]["status"], "superseded");

    let (status, superseded_by_after, forgotten_at) = memory_row(&db_path, &old_id);
    assert_eq!(status, "superseded");
    assert!(superseded_by_after.is_some());
    assert!(forgotten_at.is_none());
}

#[test]
fn restore_on_a_never_forgotten_memory_is_a_noop() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "never forgotten");

    let out = bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--restore",
            &id,
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"][0]["status"], "active");
    assert_eq!(v["results"][0]["changed"], false);

    let (status, _, _) = memory_row(&db_path, &id);
    assert_eq!(status, "active");
}

#[test]
fn purge_response_states_it_is_destructive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "purge me");

    let out = bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), "--purge", &id])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["mode"], "purge");
    assert_eq!(v["destructive"], true);
    assert_eq!(v["results"][0]["status"], "purged");
}

#[test]
fn purge_removes_memory_chunks_fts_and_metadata_rows() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "unique purge target phrase glorbnix",
            "--meta",
            "kind=decision",
        ])
        .assert()
        .success();
    let out = bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "lexical",
            "--query",
            "glorbnix",
        ])
        .assert()
        .success();
    let id = parse_single_json(&out.get_output().stdout)["results"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), "--purge", &id])
        .assert()
        .success();

    let conn = Connection::open(&db_path).unwrap();
    let mem_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mem_count, 0);
    let chunk_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE memory_id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(chunk_count, 0);
    let meta_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_meta WHERE memory_id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(meta_count, 0);

    // FTS MATCH must no longer find the purged content (architecture.md
    // §15 cascade requirement, project-plan.md S4 test list).
    let out = bin_in(dir.path())
        .args([
            "ask",
            "--db",
            db_path.to_str().unwrap(),
            "--mode",
            "lexical",
            "--query",
            "glorbnix",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"], serde_json::json!([]));
}

#[test]
fn purging_a_memory_that_is_pointed_at_by_superseded_by_does_not_error() {
    // Purging the memory another memory's `superseded_by` points at must
    // not trip the memories(id) foreign key (a worker judgment call: the
    // dangling reference is nulled first -- see src/forget.rs::purge_ids).
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let old_id = save_id(dir.path(), &db_path, "old, will be superseded");
    let new_out = bin_in(dir.path())
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            "new, supersedes old",
            "--supersedes",
            &old_id,
        ])
        .assert()
        .success();
    let new_id = parse_single_json(&new_out.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--purge",
            &new_id,
        ])
        .assert()
        .success();

    let (status, superseded_by, _) = memory_row(&db_path, &old_id);
    assert_eq!(status, "superseded");
    assert!(superseded_by.is_none(), "dangling reference must be nulled");
}

#[test]
fn unknown_id_is_not_found_and_nothing_else_changes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let real_id = save_id(
        dir.path(),
        &db_path,
        "real memory, should survive untouched",
    );

    bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            &real_id,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        ])
        .assert()
        .code(4);

    // All-or-nothing: the real id must be untouched.
    let (status, _, _) = memory_row(&db_path, &real_id);
    assert_eq!(status, "active");
}

#[test]
fn unknown_id_not_found_applies_to_purge_and_restore_too() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");

    bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--purge",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        ])
        .assert()
        .code(4);

    bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--restore",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        ])
        .assert()
        .code(4);
}

#[test]
fn purge_and_restore_flags_together_are_a_usage_error() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id = save_id(dir.path(), &db_path, "flag conflict target");
    bin_in(dir.path())
        .args([
            "forget",
            "--db",
            db_path.to_str().unwrap(),
            "--purge",
            "--restore",
            &id,
        ])
        .assert()
        .code(2);
}

#[test]
fn forget_with_no_ids_is_a_usage_error() {
    let dir = tempdir().unwrap();
    bin_in(dir.path()).args(["forget"]).assert().code(2);
}

#[test]
fn multi_id_forget_processes_all_given_ids() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("m.db");
    let id_a = save_id(dir.path(), &db_path, "multi forget a");
    let id_b = save_id(dir.path(), &db_path, "multi forget b");

    let out = bin_in(dir.path())
        .args(["forget", "--db", db_path.to_str().unwrap(), &id_a, &id_b])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["count"], 2);

    let (status_a, _, _) = memory_row(&db_path, &id_a);
    let (status_b, _, _) = memory_row(&db_path, &id_b);
    assert_eq!(status_a, "forgotten");
    assert_eq!(status_b, "forgotten");
}
