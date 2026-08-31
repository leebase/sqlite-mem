//! `save` contract tests (project-plan.md S2): exact JSON schema, dedup
//! idempotency, supersession transitions, `--if-new`, validation caps.

mod common;

use common::{bin_in, parse_single_json};
use predicates::prelude::*;
use rusqlite::Connection;
use tempfile::tempdir;

/// `(status, superseded_by)` for `id`, read directly from the db file --
/// used where the CLI response alone can't distinguish "nothing changed"
/// from "changed back to the same value".
fn memory_state(db_path: &std::path::Path, id: &str) -> (String, Option<String>) {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT status, superseded_by FROM memories WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

#[test]
fn save_stdin_produces_documented_json_schema() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--stdin", "--meta", "kind=decision"])
        .write_stdin("fact")
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "save");
    assert_eq!(v["deduplicated"], false);
    assert_eq!(v["chunks"], 1);
    assert!(v["id"].as_str().unwrap().len() >= 20, "ULID-shaped id");
    assert!(v["content_hash"].as_str().unwrap().starts_with("sha256:"));
    assert!(v["created_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(v["superseded"], serde_json::json!([]));
    assert_eq!(v["embedder"]["id"], "granite-embedding-small-english-r2");
    assert_eq!(v["embedder"]["dims"], 384);
}

#[test]
fn save_content_flag_also_works() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "hello there"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
}

#[test]
fn source_flag_is_accepted_and_does_not_affect_response_shape() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args([
            "save",
            "--content",
            "sourced fact",
            "--source",
            "decisions.md#D012",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["deduplicated"], false);
}

#[test]
fn dedup_returns_existing_id_and_flags_it() {
    let dir = tempdir().unwrap();
    let first = bin_in(dir.path())
        .args(["save", "--content", "same content twice"])
        .assert()
        .success();
    let first_json = parse_single_json(&first.get_output().stdout);
    let first_id = first_json["id"].as_str().unwrap().to_string();

    let second = bin_in(dir.path())
        .args(["save", "--content", "same content twice"])
        .assert()
        .success();
    let second_json = parse_single_json(&second.get_output().stdout);

    assert_eq!(second_json["deduplicated"], true);
    assert_eq!(second_json["id"], first_id);
}

#[test]
fn dedup_is_idempotent_across_many_retries() {
    let dir = tempdir().unwrap();
    let mut ids = Vec::new();
    for _ in 0..5 {
        let out = bin_in(dir.path())
            .args(["save", "--content", "retry loop content"])
            .assert()
            .success();
        let v = parse_single_json(&out.get_output().stdout);
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    assert!(
        ids.windows(2).all(|w| w[0] == w[1]),
        "every retry returns the same id: {ids:?}"
    );
}

#[test]
fn if_new_fails_on_duplicate_content() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "unique-ish content"])
        .assert()
        .success();

    bin_in(dir.path())
        .args(["save", "--content", "unique-ish content", "--if-new"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"ok\":false"))
        .stdout(predicate::str::contains("not_new"));
}

#[test]
fn if_new_succeeds_on_genuinely_new_content() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "first fact", "--if-new"])
        .assert()
        .success();
}

#[test]
fn supersedes_marks_target_superseded() {
    let dir = tempdir().unwrap();
    let old = bin_in(dir.path())
        .args(["save", "--content", "old decision text"])
        .assert()
        .success();
    let old_id = parse_single_json(&old.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let new = bin_in(dir.path())
        .args([
            "save",
            "--content",
            "new decision text",
            "--supersedes",
            &old_id,
        ])
        .assert()
        .success();
    let new_json = parse_single_json(&new.get_output().stdout);
    assert_eq!(new_json["superseded"], serde_json::json!([old_id]));
}

// architecture.md §11.2 (amended post-S2-review): a dedup hit must not
// silently drop the caller's retire-intent. `--supersedes` targets are
// still retired on a dedup hit, pointing `superseded_by` at the *existing*
// memory's id, in a transaction; self-supersession is ignored.

#[test]
fn dedup_with_supersedes_retires_target_at_existing_id() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join(".sqlite-mem").join("memory.db");

    let old = bin_in(dir.path())
        .args(["save", "--content", "old truth"])
        .assert()
        .success();
    let old_id = parse_single_json(&old.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // First save of "new truth" -- not a dedup hit, establishes the id that
    // the later retried/duplicate save will dedup onto.
    let first = bin_in(dir.path())
        .args(["save", "--content", "new truth"])
        .assert()
        .success();
    let existing_json = parse_single_json(&first.get_output().stdout);
    assert_eq!(existing_json["deduplicated"], false);
    let existing_id = existing_json["id"].as_str().unwrap().to_string();

    // Retry: identical content (dedup hit) + --supersedes the old memory.
    let retry = bin_in(dir.path())
        .args(["save", "--content", "new truth", "--supersedes", &old_id])
        .assert()
        .success();
    let retry_json = parse_single_json(&retry.get_output().stdout);
    assert_eq!(retry_json["deduplicated"], true);
    assert_eq!(retry_json["id"], existing_id);
    assert_eq!(retry_json["superseded"], serde_json::json!([old_id]));

    let (status, superseded_by) = memory_state(&db_path, &old_id);
    assert_eq!(status, "superseded");
    assert_eq!(superseded_by.as_deref(), Some(existing_id.as_str()));
}

#[test]
fn retried_dedup_supersedes_is_idempotent_on_second_retry() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join(".sqlite-mem").join("memory.db");

    let old = bin_in(dir.path())
        .args(["save", "--content", "old truth 2"])
        .assert()
        .success();
    let old_id = parse_single_json(&old.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = bin_in(dir.path())
        .args(["save", "--content", "new truth 2"])
        .assert()
        .success();
    let existing_id = parse_single_json(&first.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // First retry: retires old_id, as above.
    bin_in(dir.path())
        .args(["save", "--content", "new truth 2", "--supersedes", &old_id])
        .assert()
        .success();

    // Second retry, byte-identical invocation: old_id is no longer active,
    // so this must change nothing further -- reported `superseded: []`,
    // and old_id's row untouched (still superseded by the same id).
    let second_retry = bin_in(dir.path())
        .args(["save", "--content", "new truth 2", "--supersedes", &old_id])
        .assert()
        .success();
    let second_json = parse_single_json(&second_retry.get_output().stdout);
    assert_eq!(second_json["deduplicated"], true);
    assert_eq!(second_json["id"], existing_id);
    assert_eq!(second_json["superseded"], serde_json::json!([]));

    let (status, superseded_by) = memory_state(&db_path, &old_id);
    assert_eq!(status, "superseded");
    assert_eq!(superseded_by.as_deref(), Some(existing_id.as_str()));
}

#[test]
fn self_supersession_on_dedup_is_a_noop() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join(".sqlite-mem").join("memory.db");

    let first = bin_in(dir.path())
        .args(["save", "--content", "self-referential content"])
        .assert()
        .success();
    let id = parse_single_json(&first.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Dedup hit that supersedes its own (existing) id.
    let retry = bin_in(dir.path())
        .args([
            "save",
            "--content",
            "self-referential content",
            "--supersedes",
            &id,
        ])
        .assert()
        .success();
    let retry_json = parse_single_json(&retry.get_output().stdout);
    assert_eq!(retry_json["deduplicated"], true);
    assert_eq!(retry_json["id"], id);
    assert_eq!(retry_json["superseded"], serde_json::json!([]));

    let (status, superseded_by) = memory_state(&db_path, &id);
    assert_eq!(status, "active");
    assert_eq!(superseded_by, None);
}

#[test]
fn supersedes_unknown_id_is_silently_skipped() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "text", "--supersedes", "not-a-real-id"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["superseded"], serde_json::json!([]));
}

#[test]
fn oversized_content_is_exit_3_with_validation_code() {
    // Fed via --stdin, not --content: a 1MiB+ argv entry can exceed the
    // OS's ARG_MAX and fail exec() before this binary even runs, which
    // would test the shell, not the validation cap.
    let dir = tempdir().unwrap();
    let big = "a".repeat(1_048_577);
    bin_in(dir.path())
        .args(["save", "--stdin"])
        .write_stdin(big)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("input_too_large"));
}

#[test]
fn empty_content_is_exit_3() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "   "])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("empty_content"));
}

#[test]
fn bad_meta_key_is_exit_3() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "x", "--meta", "bad key=v"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("invalid_meta_key"));
}

#[test]
fn too_many_meta_pairs_is_exit_3() {
    let dir = tempdir().unwrap();
    let mut args = vec!["save".to_string(), "--content".to_string(), "x".to_string()];
    for i in 0..65 {
        args.push("--meta".to_string());
        args.push(format!("k{i}=v"));
    }
    bin_in(dir.path())
        .args(&args)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("too_many_meta_pairs"));
}

#[test]
fn missing_content_and_stdin_is_usage_error_exit_2() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("\"ok\":false"));
}

#[test]
fn both_content_and_stdin_is_usage_error_exit_2() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "x", "--stdin"])
        .assert()
        .code(2);
}

#[test]
fn unknown_verb_is_clap_usage_error_exit_2() {
    let dir = tempdir().unwrap();
    bin_in(dir.path()).args(["frobnicate"]).assert().code(2);
}

#[test]
fn creates_default_db_at_dot_sqlite_mem_with_0700_0600() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "default path test"])
        .assert()
        .success();

    let db_path = dir.path().join(".sqlite-mem").join("memory.db");
    assert!(db_path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir_mode = std::fs::metadata(dir.path().join(".sqlite-mem"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
        let file_mode = std::fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
    }
}

#[test]
fn explicit_db_flag_with_missing_parent_dir_is_db_error_exit_5() {
    let dir = tempdir().unwrap();
    let bad_path = dir.path().join("nonexistent-subdir").join("memory.db");
    bin_in(dir.path())
        .args(["save", "--content", "x", "--db", bad_path.to_str().unwrap()])
        .assert()
        .code(5);
}
