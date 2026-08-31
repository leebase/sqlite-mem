//! `ask` contract tests (project-plan.md S3): JSON schema shape, hybrid/
//! lexical/semantic modes, filters, min-score, determinism, collapse-
//! before-truncate, and the empty-DB/no-results envelopes.
//!
//! The Fixed test embedder (`tests/common`) is a hash-based one-hot fake
//! with no semantic meaning, so these tests exercise pipeline *mechanics*
//! (ranking math, filtering, collapsing, JSON shape, determinism), not
//! retrieval *quality*. The quality acceptance scenario (Mastra vs. "that
//! agent framework") requires the real granite model and is run manually
//! per project-plan.md S3's acceptance criteria, not in this automated
//! suite.

mod common;

use common::{bin_in, parse_single_json};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use tempfile::tempdir;

fn db_path(dir: &Path) -> std::path::PathBuf {
    dir.join(".sqlite-mem").join("memory.db")
}

/// A 384-dim all-zero embedding blob -- fine for tests that only exercise
/// `--mode lexical` and never touch the semantic leg.
fn zero_embedding_blob() -> Vec<u8> {
    vec![0u8; 384 * 4]
}

/// Directly inserts a memory + its chunks via raw SQL against the real
/// schema (bypassing `save`'s chunker/embedder) so tests can control exact
/// FTS document text and length -- used by the collapse-before-truncate
/// test, which needs precise control over bm25 ranking that `save`'s
/// paragraph chunker can't give deterministically.
fn seed_memory(conn: &Connection, id: &str, chunks: &[&str]) {
    conn.execute(
        "INSERT INTO memories (id, content, content_hash, source, created_at, status, superseded_by, forgotten_at)
         VALUES (?1, ?2, ?3, NULL, '2026-01-01T00:00:00Z', 'active', NULL, NULL)",
        params![id, format!("content for {id}"), format!("hash-{id}")],
    )
    .unwrap();
    for (idx, text) in chunks.iter().enumerate() {
        conn.execute(
            "INSERT INTO chunks (id, memory_id, idx, text, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                format!("{id}:{idx}"),
                id,
                idx as i64,
                text,
                zero_embedding_blob()
            ],
        )
        .unwrap();
    }
}

#[test]
fn empty_db_returns_ok_true_and_empty_results() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["ask", "--query", "anything at all"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "ask");
    assert_eq!(v["mode"], "hybrid");
    assert_eq!(v["query"], "anything at all");
    assert_eq!(v["results"], serde_json::json!([]));
    assert_eq!(v["stats"]["candidates"], 0);
    assert_eq!(v["stats"]["returned"], 0);
}

#[test]
fn response_shape_matches_the_documented_schema() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "We rejected Mastra because of durability concerns.",
            "--meta",
            "project=factory",
            "--meta",
            "kind=decision",
            "--source",
            "decisions.md#D012",
        ])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args(["ask", "--query", "Mastra durability"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["op"], "ask");
    assert_eq!(v["mode"], "hybrid");
    assert_eq!(v["query"], "Mastra durability");

    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert!(r["id"].as_str().unwrap().len() >= 20, "ULID-shaped id");
    assert_eq!(
        r["content"],
        "We rejected Mastra because of durability concerns."
    );
    assert!(r["score"].as_f64().unwrap() > 0.0);
    assert!(r["ranks"]["lexical"].as_u64().is_some());
    assert!(r["ranks"]["semantic"].as_u64().is_some());
    assert_eq!(r["metadata"]["project"], "factory");
    assert_eq!(r["metadata"]["kind"], "decision");
    assert!(r["system"]["created_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(r["system"]["source"], "decisions.md#D012");
    assert_eq!(r["system"]["status"], "active");
    assert!(r["system"]["content_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(v["stats"]["returned"], 1);
    assert!(v["stats"]["candidates"].as_u64().unwrap() >= 1);
}

#[test]
fn source_is_null_when_not_supplied() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "no source on this one"])
        .assert()
        .success();
    let out = bin_in(dir.path())
        .args(["ask", "--query", "no source on this one"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["results"][0]["system"]["source"], serde_json::Value::Null);
}

#[test]
fn metadata_keys_are_serialized_in_sorted_order() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "sorted metadata check text",
            "--meta",
            "zeta=last",
            "--meta",
            "alpha=first",
            "--meta",
            "mid=middle",
        ])
        .assert()
        .success();
    let out = bin_in(dir.path())
        .args(["ask", "--query", "sorted metadata check text"])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    let meta = v["results"][0]["metadata"].as_object().unwrap();
    let keys: Vec<&str> = meta.keys().map(|k| k.as_str()).collect();
    assert_eq!(keys, vec!["alpha", "mid", "zeta"]);
}

#[test]
fn determinism_two_runs_are_byte_identical_except_elapsed_ms() {
    let dir = tempdir().unwrap();
    for i in 0..6 {
        bin_in(dir.path())
            .args([
                "save",
                "--content",
                &format!("determinism corpus memory {i}"),
            ])
            .assert()
            .success();
    }

    let strip_elapsed = |bytes: &[u8]| -> String {
        let text = std::str::from_utf8(bytes).unwrap();
        let v = parse_single_json(bytes);
        assert!(v["stats"]["elapsed_ms"].is_number());
        // Replace the elapsed_ms value textually so the rest of the byte
        // stream is compared as-is (field order/spacing included).
        regex_lite_replace_elapsed(text)
    };

    fn regex_lite_replace_elapsed(text: &str) -> String {
        // No regex dependency needed: elapsed_ms is always the final field
        // before the closing braces, `"elapsed_ms":<digits>`.
        let needle = "\"elapsed_ms\":";
        let start = text.find(needle).expect("elapsed_ms present") + needle.len();
        let rest = &text[start..];
        let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        format!("{}{}{}", &text[..start], "N", &text[start + digit_len..])
    }

    let first = bin_in(dir.path())
        .args(["ask", "--query", "determinism corpus memory"])
        .assert()
        .success();
    let second = bin_in(dir.path())
        .args(["ask", "--query", "determinism corpus memory"])
        .assert()
        .success();

    let a = strip_elapsed(&first.get_output().stdout);
    let b = strip_elapsed(&second.get_output().stdout);
    assert_eq!(a, b, "stdout must be byte-identical except elapsed_ms");
}

#[test]
fn lexical_mode_works_even_when_embeddings_are_corrupted() {
    let dir = tempdir().unwrap();
    let saved = bin_in(dir.path())
        .args(["save", "--content", "octopus banjo festival announcement"])
        .assert()
        .success();
    let id = parse_single_json(&saved.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    {
        let conn = Connection::open(db_path(dir.path())).unwrap();
        // Corrupt the embedding: 3 garbage bytes, not even a multiple of 4.
        conn.execute(
            "UPDATE chunks SET embedding = ?1 WHERE memory_id = ?2",
            params![vec![0xAB_u8, 0xCD, 0xEF], id],
        )
        .unwrap();
    }

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "octopus banjo festival",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["results"][0]["id"], id);

    // The semantic leg must not panic/crash on the same corrupted DB either
    // (undefined ranking, but no crash -- non-goal to assert its content).
    bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "semantic",
            "--query",
            "octopus banjo festival",
        ])
        .assert()
        .success();
}

#[test]
fn semantic_mode_skips_fts_and_matches_by_embedding_only() {
    let dir = tempdir().unwrap();
    // The Fixed test embedder is a deterministic hash of the exact text, so
    // querying with the exact same string as the saved content guarantees
    // cosine similarity 1.0 (rank 1), with zero lexical vocabulary overlap
    // required for the assertion itself.
    let content = "quantum flapdoodle penguin zephyr unrelated to lexical terms";
    bin_in(dir.path())
        .args(["save", "--content", content])
        .assert()
        .success();
    bin_in(dir.path())
        .args(["save", "--content", "a completely different decoy memory"])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args(["ask", "--mode", "semantic", "--query", content])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["mode"], "semantic");
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0]["content"], content);
    assert_eq!(results[0]["ranks"]["semantic"], 1);
    // `--mode semantic` never runs the lexical leg, so the field is absent.
    assert!(results[0]["ranks"].get("lexical").is_none());
}

#[test]
fn where_equality_filters_to_matching_memories() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "filter target memory alpha",
            "--meta",
            "kind=decision",
        ])
        .assert()
        .success();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "filter target memory beta",
            "--meta",
            "kind=note",
        ])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "filter target memory",
            "--where",
            "kind=decision",
            "--k",
            "10",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["content"], "filter target memory alpha");
}

#[test]
fn where_not_equal_excludes_the_matching_value() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "ne filter memory alpha",
            "--meta",
            "kind=decision",
        ])
        .assert()
        .success();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "ne filter memory beta",
            "--meta",
            "kind=note",
        ])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "ne filter memory",
            "--where",
            "kind!=decision",
            "--k",
            "10",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["content"], "ne filter memory beta");
}

#[test]
fn where_existence_filters_by_key_presence() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "exists filter memory alpha",
            "--meta",
            "kind=decision",
        ])
        .assert()
        .success();
    bin_in(dir.path())
        .args(["save", "--content", "exists filter memory beta"])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "exists filter memory",
            "--where",
            "kind=*",
            "--k",
            "10",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["content"], "exists filter memory alpha");
}

#[test]
fn superseded_excluded_by_default_included_with_flag() {
    let dir = tempdir().unwrap();
    let old = bin_in(dir.path())
        .args(["save", "--content", "supersede chain old truth here"])
        .assert()
        .success();
    let old_id = parse_single_json(&old.get_output().stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();
    bin_in(dir.path())
        .args([
            "save",
            "--content",
            "supersede chain new truth here",
            "--supersedes",
            &old_id,
        ])
        .assert()
        .success();

    let default_out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "supersede chain",
            "--k",
            "10",
        ])
        .assert()
        .success();
    let default_v = parse_single_json(&default_out.get_output().stdout);
    assert_eq!(default_v["results"].as_array().unwrap().len(), 1);
    assert_eq!(default_v["results"][0]["system"]["status"], "active");

    let included_out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "supersede chain",
            "--include-superseded",
            "--k",
            "10",
        ])
        .assert()
        .success();
    let included_v = parse_single_json(&included_out.get_output().stdout);
    assert_eq!(included_v["results"].as_array().unwrap().len(), 2);
}

#[test]
fn min_score_excludes_results_below_the_threshold() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "min score threshold memory text"])
        .assert()
        .success();

    let unfiltered = bin_in(dir.path())
        .args(["ask", "--mode", "lexical", "--query", "min score threshold"])
        .assert()
        .success();
    let unfiltered_v = parse_single_json(&unfiltered.get_output().stdout);
    assert_eq!(unfiltered_v["results"].as_array().unwrap().len(), 1);

    let filtered = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "min score threshold",
            "--min-score",
            "1.0",
        ])
        .assert()
        .success();
    let filtered_v = parse_single_json(&filtered.get_output().stdout);
    assert_eq!(filtered_v["results"], serde_json::json!([]));
    assert_eq!(filtered_v["stats"]["candidates"], 1);
    assert_eq!(filtered_v["stats"]["returned"], 0);
}

#[test]
fn no_results_when_where_filter_matches_nothing() {
    let dir = tempdir().unwrap();
    bin_in(dir.path())
        .args(["save", "--content", "some memory with no matching meta"])
        .assert()
        .success();

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--query",
            "some memory",
            "--where",
            "kind=nonexistent-value",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["results"], serde_json::json!([]));
}

#[test]
fn collapse_before_truncate_keeps_k_distinct_memories() {
    let dir = tempdir().unwrap();
    // Create the schema first via a no-op `info` call, then seed directly
    // with hand-picked short/dense text so bm25 ranking is under test
    // control: memory A's two chunks are the two shortest, highest-
    // term-frequency documents in the corpus and are expected to occupy
    // the best two chunk-level bm25 ranks, ahead of any single decoy.
    bin_in(dir.path()).args(["info"]).assert().success();
    let conn = Connection::open(db_path(dir.path())).unwrap();

    seed_memory(
        &conn,
        "memA",
        &[
            "zephyrquokka zephyrquokka",
            "zephyrquokka zephyrquokka zephyrquokka",
        ],
    );
    seed_memory(
        &conn,
        "memB",
        &["the zephyrquokka appears once in this decoy sentence"],
    );
    seed_memory(
        &conn,
        "memC",
        &["another mention of zephyrquokka happens right here"],
    );
    seed_memory(
        &conn,
        "memD",
        &["zephyrquokka shows up again in this longer decoy passage today"],
    );
    drop(conn);

    let out = bin_in(dir.path())
        .args([
            "ask",
            "--mode",
            "lexical",
            "--query",
            "zephyrquokka",
            "--k",
            "2",
        ])
        .assert()
        .success();
    let v = parse_single_json(&out.get_output().stdout);

    // 5 chunks total (2 from A + 1 each from B/C/D) all match "zephyrquokka".
    assert_eq!(v["stats"]["candidates"], 5);

    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "k=2 must return exactly 2 results");

    let ids: HashSet<&str> = results.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids.len(),
        2,
        "the 2 results must be 2 DISTINCT memories, never the same memory twice: {results:?}"
    );
    assert!(
        ids.contains("memA"),
        "memory A (whose 2 chunks both rank near the top) must still appear in top-k, \
         not be silently represented twice while a distinct memory is crowded out: {results:?}"
    );
    // Whichever of A's two chunks won, only ONE of its ranks is reported --
    // never both, since collapse keeps a single representative chunk.
    let a_result = results.iter().find(|r| r["id"] == "memA").unwrap();
    assert!(a_result["ranks"]["lexical"].as_u64().is_some());
}
