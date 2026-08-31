//! `info` (basic) contract tests (project-plan.md S2).

mod common;

use common::{bin_in, parse_single_json};
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
