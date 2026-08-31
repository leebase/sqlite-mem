//! Multi-process concurrency stress test (architecture.md §16,
//! project-plan.md S4 acceptance gate): 8 writer processes x 100 saves
//! each + 4 concurrent asker processes against ONE db. Acceptance: zero
//! failed operations, zero busy errors surfacing to callers, and
//! `info --verify` passes afterward.
//!
//! Each "writer"/"asker" is a thread in this test process that drives its
//! own sequential stream of real `sqlite-mem` child-process invocations
//! (architecture.md's unit of concurrency is the OS process -- sqlite-mem
//! is a transient one-shot CLI, so "8 concurrent writers" means 8
//! concurrently-running process lineages, not one process doing 100 saves
//! internally); the 8 writer lineages and 4 asker lineages all run at the
//! same time against the same db file.
//!
//! `#[ignore]`d by default -- this is deliberately slow (roughly a
//! thousand process spawns). Run explicitly:
//! `cargo test --no-default-features --features test-support --test concurrency_stress -- --ignored --nocapture`
//! project-plan.md's S4 acceptance criterion is "stress test green 10
//! consecutive runs"; the worker report records however many of those fit
//! in the time budget.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

const WRITERS: usize = 8;
const SAVES_PER_WRITER: usize = 100;
const ASKERS: usize = 4;
const ASKS_PER_ASKER: usize = 50;

fn sqlite_mem_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sqlite-mem"));
    cmd.env("SQLITE_MEM_FIXED_EMBEDDER", "1");
    cmd.env_remove("SQLITE_MEM_DB");
    cmd
}

fn check_ok(out: &Output, op: &str) -> Result<(), String> {
    if !out.status.success() {
        return Err(format!(
            "{op} exited {:?} (busy/lock errors surface as a non-zero exit here)\nstdout: {}\nstderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| {
        format!(
            "{op} stdout was not valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })?;
    if v["ok"] != true {
        return Err(format!("{op} returned ok:false: {v}"));
    }
    Ok(())
}

fn run_save(db_path: &Path, content: &str) -> Result<(), String> {
    let out = sqlite_mem_cmd()
        .args([
            "save",
            "--db",
            db_path.to_str().unwrap(),
            "--content",
            content,
        ])
        .output()
        .map_err(|e| format!("spawn failed: {e}"))?;
    check_ok(&out, "save")
}

fn run_ask(db_path: &Path, query: &str) -> Result<(), String> {
    let out = sqlite_mem_cmd()
        .args(["ask", "--db", db_path.to_str().unwrap(), "--query", query])
        .output()
        .map_err(|e| format!("spawn failed: {e}"))?;
    check_ok(&out, "ask")
}

#[test]
#[ignore]
fn eight_writers_and_four_askers_against_one_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path: PathBuf = dir.path().join("stress.db");

    // Seed the schema/first row up front so all 12 threads below are
    // racing steady-state WAL concurrency, not the one-time
    // schema-creation path (that race is covered by tests/migrations.rs).
    run_save(&db_path, "seed memory for the concurrency stress test").unwrap();

    let failures: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for w in 0..WRITERS {
        let db_path = db_path.clone();
        let failures = Arc::clone(&failures);
        handles.push(std::thread::spawn(move || {
            for i in 0..SAVES_PER_WRITER {
                if let Err(e) = run_save(&db_path, &format!("writer {w} save {i}")) {
                    failures.lock().unwrap().push(e);
                }
            }
        }));
    }

    for a in 0..ASKERS {
        let db_path = db_path.clone();
        let failures = Arc::clone(&failures);
        handles.push(std::thread::spawn(move || {
            for i in 0..ASKS_PER_ASKER {
                if let Err(e) = run_ask(&db_path, &format!("query {a} {i} writer save")) {
                    failures.lock().unwrap().push(e);
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("stress thread panicked");
    }

    let failures = failures.lock().unwrap();
    assert!(
        failures.is_empty(),
        "{} of {} operations failed (expected zero -- architecture.md §16):\n{}",
        failures.len(),
        WRITERS * SAVES_PER_WRITER + ASKERS * ASKS_PER_ASKER,
        failures.join("\n---\n")
    );
    drop(failures);

    let verify_out = sqlite_mem_cmd()
        .args(["info", "--verify", "--db", db_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify_out.status.success(),
        "info --verify failed (exit {:?}) after the stress run:\n{}",
        verify_out.status.code(),
        String::from_utf8_lossy(&verify_out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&verify_out.stdout).unwrap();
    assert_eq!(v["ok"], true, "verify checks after stress run: {v}");

    // Sanity: every save actually landed (8*100 writer saves + 1 seed).
    let info_out = sqlite_mem_cmd()
        .args(["info", "--db", db_path.to_str().unwrap()])
        .output()
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&info_out.stdout).unwrap();
    assert_eq!(
        info["counts"]["active"],
        (WRITERS * SAVES_PER_WRITER + 1) as i64
    );
}
