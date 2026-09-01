//! Resolves the embedding model directory for the `embed-model` feature's
//! `include_bytes!` calls in `src/embed/mod.rs` (architecture.md §8: "one
//! file, no extraction, no first run, no network").
//!
//! `include_bytes!` needs a compile-time-literal path, so it can't read an
//! arbitrary env var directly. This script resolves the model directory,
//! validates the three required files exist, and re-exports the resolved
//! path as a `rustc-env` (`SQLITE_MEM_EMBED_MODEL_DIR`) that
//! `src/embed/mod.rs` consumes via
//! `include_bytes!(concat!(env!("SQLITE_MEM_EMBED_MODEL_DIR"), "/model.f16.safetensors"))`.
//!
//! Resolution order:
//!   1. `SQLITE_MEM_EMBED_MODEL_DIR` set in the build environment -- this is
//!      how release CI points the build at the model it just downloaded
//!      from the pinned HF revision, verified by sha256, and converted to
//!      f16 (architecture.md §9; `.github/workflows/release.yml`). This
//!      script never downloads or converts anything itself -- no network
//!      access, ever, from a cargo build.
//!   2. Otherwise, the S1 spike's already-converted copy at
//!      `spike/embed-parity/models/granite/` -- lets
//!      `cargo build --features embed-model` work out of the box in this
//!      repo for local/dev use without any env var. (Read-only reference;
//!      this script and the product never write into `spike/`.)
//!
//! Only runs when the `embed-model` feature is enabled (checked via the
//! `CARGO_FEATURE_EMBED_MODEL` env var Cargo sets for us); a no-op
//! otherwise, so `model-sidecar` (which resolves its own model directory at
//! *runtime* via `SQLITE_MEM_MODEL_DIR`, see `src/embed/mod.rs`) and
//! `test-support` builds pay nothing for this script.

use std::env;
use std::path::PathBuf;

fn main() {
    if env::var_os("CARGO_FEATURE_EMBED_MODEL").is_none() {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"));
    let default_dir = manifest_dir.join("spike/embed-parity/models/granite");

    let model_dir = match env::var("SQLITE_MEM_EMBED_MODEL_DIR") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => default_dir,
    };

    let required = ["model.f16.safetensors", "tokenizer.json", "config.json"];
    for name in required {
        let path = model_dir.join(name);
        if !path.is_file() {
            panic!(
                "sqlite-mem: the `embed-model` feature needs {name} at {} (missing).\n\
                 Set SQLITE_MEM_EMBED_MODEL_DIR to a directory containing \
                 model.f16.safetensors, tokenizer.json, and config.json -- release CI \
                 populates this after downloading the pinned model revision and \
                 converting it to f16 (see .github/workflows/release.yml). For local \
                 builds, the default is spike/embed-parity/models/granite, which the \
                 S1 spike already populated.",
                path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // rustc's file-loading for include!/include_bytes! accepts forward
    // slashes on every platform (including Windows), so normalize here
    // rather than emitting a path that only works on the OS that built it.
    let normalized = model_dir.display().to_string().replace('\\', "/");
    println!("cargo:rustc-env=SQLITE_MEM_EMBED_MODEL_DIR={normalized}");
    println!("cargo:rerun-if-env-changed=SQLITE_MEM_EMBED_MODEL_DIR");
}
