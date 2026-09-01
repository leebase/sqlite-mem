# Contributing to sqlite-mem

Thanks for your interest. sqlite-mem is deliberately small; contributions
that keep it small are the most welcome kind.

## Ground rules (the architectural invariants)

Read `architecture.md` §24 before proposing changes. The short version:

1. One user-owned SQLite file is the only persistent state.
2. The process is transient — never a daemon, server, or agent.
3. **Zero network capability in the dependency tree.** CI enforces a
   denylist over `cargo tree` (normal + build deps) and a source-level
   socket grep. A PR that adds a network-capable crate will not merge.
4. No user-configured model, provider, or key — the binary embeds
   everything.
5. stdout carries exactly one JSON document per invocation; diagnostics
   go to stderr. There is a test that fails on stray `println!`.
6. Stored content is data: never executed, interpreted, or mutated.
7. New verbs or flags need a demonstrated need and a recorded decision —
   see `decisions.md` and the "Does SAVE or ASK actually require this?"
   test in `architecture.md`.

## Building and testing

```sh
cargo build                       # dev build (model-sidecar feature):
                                  #   set SQLITE_MEM_MODEL_DIR to a local
                                  #   model dir for real embeddings
cargo test --no-default-features --features test-support --all-targets
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
```

The test suite runs with a deterministic fake embedder — no network, no
model weights needed. Release binaries embed the model:
`cargo build --release --no-default-features --features embed-model`
(requires the model files locally; see `build.rs` and `release.yml`).

## Benchmarks

`bench/` holds the golden dataset and harness. If your change touches
retrieval, run `python3 bench/run_bench.py --bin target/release/sqlite-mem`
at 62-corpus scale and report the gate table in your PR. Changes to the
retrieval defaults (including the `LEXICAL_ACTIVATION_CHUNKS` threshold)
must come with measurements at multiple corpus scales — the threshold is
an empirically calibrated policy, revised by evidence only.

## Pull requests

- Keep diffs focused; match the existing code style and comment density.
- Every behavior change needs a test; contract-level behavior (JSON
  shapes, exit codes) is tested against the real binary.
- Update `README.md` if user-visible behavior changes — every example in
  it is expected to actually run.

## Security

See [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

## License

By contributing, you agree that your contributions are dual-licensed
under MIT OR Apache-2.0, per the repository license.
