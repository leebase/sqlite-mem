//! Shared integration-test helpers.
//!
//! Every test in this crate exercises the real `sqlite-mem` binary
//! (project-plan.md S2: "Write integration tests that exercise the real
//! binary"). `SQLITE_MEM_FIXED_EMBEDDER=1` selects the deterministic
//! `test-support` embedder so these tests need neither network nor the
//! bundled model weights, regardless of which candle feature the binary
//! was additionally built with.

use assert_cmd::Command;
use std::path::Path;

/// A `Command` for the real binary, pre-armed for offline determinism.
pub fn bin() -> Command {
    let mut cmd = Command::cargo_bin("sqlite-mem").expect("binary built");
    cmd.env("SQLITE_MEM_FIXED_EMBEDDER", "1");
    cmd.env_remove("SQLITE_MEM_DB");
    cmd
}

/// A `Command` scoped to a fresh temp directory as both cwd and default DB
/// location (so parallel tests never collide on `./.sqlite-mem/memory.db`).
pub fn bin_in(dir: &Path) -> Command {
    let mut cmd = bin();
    cmd.current_dir(dir);
    cmd
}

/// Parses `bytes` as exactly one JSON document, failing loudly (with the
/// raw output) if it is not.
#[allow(dead_code)] // used by some but not all test binaries in this crate
pub fn parse_single_json(bytes: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(bytes).expect("stdout is valid UTF-8");
    serde_json::from_str(text).unwrap_or_else(|e| {
        panic!("stdout was not exactly one JSON document: {e}\n---\n{text}\n---")
    })
}
