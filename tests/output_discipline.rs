//! Standing gates that apply to every invocation, per architecture.md §17
//! and §24.5: stdout carries exactly one JSON document, and no module
//! writes to stdout except through `src/output.rs`.

mod common;

use common::bin_in;
use tempfile::tempdir;

fn assert_stdout_is_exactly_one_json_document(stdout: &[u8]) {
    let text = std::str::from_utf8(stdout).expect("stdout is valid UTF-8");
    let mut de = serde_json::Deserializer::from_str(text).into_iter::<serde_json::Value>();
    let first = de
        .next()
        .unwrap_or_else(|| panic!("stdout produced no JSON document:\n{text}"))
        .unwrap_or_else(|e| panic!("stdout's first document did not parse: {e}\n{text}"));
    assert!(
        first.is_object(),
        "top-level document must be a JSON object"
    );
    assert!(
        de.next().is_none(),
        "stdout carried more than one JSON document:\n{text}"
    );
    // Exactly one line: a single JSON document plus its trailing newline,
    // nothing else.
    assert_eq!(
        text.matches('\n').count(),
        1,
        "expected exactly one trailing newline, stdout was:\n{text}"
    );
}

#[test]
fn save_success_stdout_is_one_json_document() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "single json doc check"])
        .assert()
        .success();
    assert_stdout_is_exactly_one_json_document(&out.get_output().stdout);
}

#[test]
fn save_error_stdout_is_one_json_document() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path())
        .args(["save", "--content", "   "])
        .assert()
        .code(3);
    assert_stdout_is_exactly_one_json_document(&out.get_output().stdout);
}

#[test]
fn info_stdout_is_one_json_document() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path()).args(["info"]).assert().success();
    assert_stdout_is_exactly_one_json_document(&out.get_output().stdout);
}

#[test]
fn usage_error_stdout_is_one_json_document() {
    let dir = tempdir().unwrap();
    let out = bin_in(dir.path()).args(["save"]).assert().code(2);
    assert_stdout_is_exactly_one_json_document(&out.get_output().stdout);
}

#[test]
fn no_stray_println_in_source() {
    // Every stdout write must funnel through src/output.rs (the single
    // sink). println!/print! anywhere else in src/ would bypass the
    // envelope contract; eprintln!/dbg! anywhere would bypass "stderr via
    // tracing only" (architecture.md §18, §24.5).
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    scan_dir(&src_dir, &mut offenders);
    assert!(
        offenders.is_empty(),
        "stray print/dbg macros found:\n{}",
        offenders.join("\n")
    );
}

fn scan_dir(dir: &std::path::Path, offenders: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, offenders);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Skip comment lines (module docs above cite the macro names by
            // name, which would otherwise false-positive this scan).
            if trimmed.starts_with("//") {
                continue;
            }
            let is_output_sink = path.ends_with("src/output.rs")
                || path.file_name().unwrap() == "output.rs"
                    && path.parent().unwrap().file_name().unwrap() == "src";
            for needle in ["println!(", "print!(", "eprintln!(", "eprint!(", "dbg!("] {
                if line.contains(needle) && !is_output_sink {
                    offenders.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
}
