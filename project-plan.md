# sqlite-mem Project Plan

**Status:** Proposed plan, submitted for Lee's ratification alongside `architecture.md`

**Date:** 2026-08-31

This plan assumes `architecture.md` is ratified. It is written so implementation sprints can be delegated to lower-cost models under supervision: each sprint states objective, exact artifacts, tests, acceptance criteria, evidence, non-goals, likely failure modes, and the recommended worker tier. No sprint may begin until its predecessor's acceptance criteria are verified and recorded in `result-review.md`.

## Overall objective

Ship v1: a fully offline, single-file-friendly Rust CLI (`save`, `ask`, `forget`, `reindex`, `info`) with a bundled embedding model, hybrid FTS5+vector retrieval over one user-owned SQLite file, deterministic JSON output, portable binaries for macOS/Linux/Windows, meeting the Definition of Done in `architecture.md` §25.

## Worker-tier legend

- **T1** — strong model (architect-class): spikes, ambiguous decisions, security review.
- **T2** — mid-tier model: core implementation against explicit specs.
- **T3** — low-cost model: mechanical work with exhaustive spec (test scaffolds, CI plumbing, docs formatting).

Supervision rule: every sprint's output is reviewed against acceptance criteria by a T1 pass (or Lee) before merge.

---

## Sprint S1 — Embedding spike (MUST precede all implementation)

**Objective:** Prove the bundled-embedding path and finalize the model. This is the highest-risk unknown; nothing else is blocked-on-unknowns once it closes.

**Worker tier:** T1.

**Scope / exact artifacts:**
- `spike/embed-parity/` Rust binary: loads granite-embedding-small-english-r2 (f16 safetensors) under Candle, embeds a 100-text fixture corpus (varied lengths incl. >512 and >4096 tokens), writes vectors to JSON.
- Python reference script (spike-only; Python never enters the product) using sentence-transformers producing the same corpus's reference vectors.
- Parity report: per-text cosine similarity, worst-case, distribution.
- Same harness run for bge-small-en-v1.5 (fallback).
- Measurements: model load time from `include_bytes!` vs sidecar file, single-embed latency, binary size with weights embedded, cross-compile smoke build for `x86_64-unknown-linux-musl` and (if runner available) `aarch64-apple-darwin` + `x86_64-pc-windows-msvc`.

**Acceptance criteria:**
- Cosine ≥ 0.999 vs reference for the chosen model on all fixture texts, OR a documented failure and the fallback model passing the same bar.
- Release-profile binary with embedded weights ≤ 150MB; cold start (process spawn → first embedding) < 1.5s on a dev machine.
- Cross-compile smoke succeeds for musl.

**Evidence required:** parity report + size/latency table committed under `spike/`; entry in `result-review.md`.

**Non-goals:** no SQLite, no CLI design, no retrieval.

**Likely failure modes:** Candle ModernBERT op gaps or pooling mismatch (→ fall back to bge-small, pre-approved); tokenizer prefix/truncation mismatch (compare tokenizations first when vectors disagree); f16 quality loss (test f32 sidecar to isolate).

**Gate G1:** Model + runtime decision recorded in `decisions.md` (this is the one mid-plan decision Lee must accept, since it may invoke the fallback).

---

## Sprint S2 — Core storage + SAVE

**Objective:** Repo scaffold, schema v1, migrations, JSON output sink, and a fully working `save`.

**Worker tier:** T2 (scaffold/CI plumbing T3). Depends on S1.

**Scope / exact artifacts:**
- Cargo workspace: `src/{main,cli,output,db,embed,chunk,save}.rs` (module boundaries per `architecture.md` §10–11, §13 seams).
- Port Satchel embed module (adapted to S1's model) and chunker with its proptests; port sqlite-graphrag output-sink discipline. Record attributions in `THIRD-PARTY.md`.
- Migration runner (forward-only, `user_version`, pre-migration `.bak`) + migration 001 with full §10 DDL incl. FTS triggers.
- `save` verb complete per §11: validation caps, dedup, chunking, embedding, single-transaction insert, supersession, exact JSON response schema; `--db`/env/default path resolution with `0700`/`0600` permissions.
- `info` (basic: schema version, embedder id, counts) — needed for testing now.
- CI: fmt, clippy (deny warnings), tests, and the **network-denylist `cargo tree` gate** from day one.

**Tests:** chunker proptests green; validation-cap table tests; dedup idempotency; supersession state transitions; stdout-single-JSON + no-stray-println gates; exit codes 0/2/3/5; migration creates `.bak`; newer-schema refusal (exit 6).

**Acceptance criteria:** `echo "fact" | sqlite-mem save --stdin --meta kind=decision` produces the documented JSON against a fresh default-path DB; all tests green in CI; denylist gate active.

**Evidence:** CI run link + transcript of the CLI session in `result-review.md`.

**Non-goals:** no `ask`, no retrieval, no releases.

**Likely failure modes:** FTS trigger drift from chunk deletes (port Satchel's backfill check into `info --verify` early); embedded-weights link times annoying dev loop (keep `model-sidecar` the dev default).

---

## Sprint S3 — ASK: hybrid retrieval

**Objective:** Full retrieval pipeline per §12–13.

**Worker tier:** T2. Depends on S2.

**Scope / exact artifacts:** `src/{ask,rank,filter,vector}.rs` — allowed-ID filter resolver (`=`, `!=`, `=*`, ANDed); FTS5 leg with token-quoting sanitizer; `VectorIndex` trait + brute-force cosine impl; RRF k=60; chunk→memory collapse; (score DESC, id ASC) ordering; 5-decimal rounding; `--mode`, `--k`, status flags; exact §12 JSON schema.

**Tests:** sanitizer fuzz (arbitrary bytes never produce FTS syntax errors); RRF unit tests with hand-computed fixtures; filter-resolver SQL correctness; determinism test (two runs, byte-identical); `--mode lexical` works on a DB with zeroed embeddings; empty-DB and no-results envelopes; exit code table extended.

**Acceptance criteria:** the kernel scenario passes end-to-end — save the Mastra memory, ask "Why didn't we use that agent framework?", get it back at rank 1 in hybrid mode with correct ranks/metadata/provenance in JSON. Determinism test green.

**Evidence:** recorded CLI transcript + CI link in `result-review.md`.

**Non-goals:** no benchmarks yet, no `--prefer`, no ANN.

**Likely failure modes:** post-collapse rank instability (collapse before truncating to k); bm25 sign confusion (FTS5 bm25 is lower-is-better — negate before ranking); forgetting query-prefix conventions for the model (embed module owns prefixes, not `ask`).

---

## Sprint S4 — Lifecycle and integrity

**Objective:** `forget`/`--purge`/`--restore`, `reindex`, `info --verify`, concurrency hardening.

**Worker tier:** T2. Depends on S3.

**Scope / exact artifacts:** verbs per §15, §18–19; embedder-mismatch refusal (exit 6) + `reindex` re-embedding path with `.bak`; `info --verify` (integrity_check, FTS backfill audit, dims audit, hash spot-check); multi-process stress test binary (8 writers × 100 saves + 4 concurrent askers).

**Tests:** forget/restore/purge state machine incl. cascades to chunks/FTS/meta; reindex on a DB stamped with a fake old embedder id; verify detects a deliberately corrupted row; stress test: zero failed operations, zero busy errors surfacing to callers, DB passes verify afterward.

**Acceptance criteria:** all verbs match documented JSON/exit codes; stress test green 10 consecutive runs.

**Non-goals:** no flock layer unless the stress test fails (then adopt sqlite-graphrag's slot pattern and re-run).

**Likely failure modes:** WAL checkpoint starvation under writers (bound test duration, check WAL size); purge leaving FTS orphans (single transaction + verify audit catches it).

---

## Sprint S5 — Benchmark suite and kernel proof

**Objective:** Measure the product claim. Gates, not vibes.

**Worker tier:** T1 for dataset design and analysis; T3 for harness plumbing (modeled on rag-ferrite's `benchmark.rs`). Depends on S3 (can overlap S4).

**Scope / exact artifacts:**
- `bench/corpus/` golden dataset: ≥ 50 realistic Folder Chief distilled memories (decisions, constraints, preferences, precedents) with metadata; ≥ 30 cross-wording queries with gold relevance labels; the Mastra pair is query #1. Dataset design is T1 work — quality here determines whether the gates mean anything.
- `bench/` harness: recall@k, MRR, nDCG (unit-tested metric math); ablation matrix (lexical/semantic/hybrid × filtered/unfiltered × primary/fallback model); ops metrics (cold start, warm latency at 1K/10K/50K chunks via synthetic inflation, RSS, binary size).
- `bench/REPORT.md` with results and the model ablation conclusion.
- Token-economy measurement (reported, not gated): tokens returned by `ask` vs tokens consumed exploring an equivalent file tree for the same questions with a real harness.

**Acceptance criteria (v1 gates):** hybrid recall@5 ≥ 0.85; MRR ≥ 0.7; hybrid ≥ each single leg on both metrics; cold start < 1.5s; warm ask < 250ms at 10K chunks.

**Evidence:** `bench/REPORT.md` + reproducible `just bench` (or equivalent) command.

**Gate G2:** if gates fail on the primary model but pass on fallback (or on arctic-m-int8/`ort`), that is a decision request to Lee, not a silent swap.

**Likely failure modes:** dataset too easy (lexical alone passes — add adversarial paraphrases until it doesn't); overfitting thresholds to one corpus (hold out 20% of queries authored after the harness is frozen).

---

## Sprint S6 — Packaging, security, release

**Objective:** Five platform artifacts + security pass + docs. Depends on S4 + S5.

**Worker tier:** T2 (CI from Satchel's release.yml template; T3 for docs formatting); T1 for the security review.

**Scope / exact artifacts:**
- Release workflow: macOS arm64/x64 (signed, notarized), Linux musl x64/arm64, Windows MSVC x64; model fetched + checksummed in CI, embedded via `--features embed-model`; SHA-256 sums published.
- Cross-platform determinism run: golden benchmark byte-identical Linux↔macOS; Windows measured (loosen contract only if forced, per §26.4).
- **Security gate (T1, independent of implementing model):** audit against §20 checklist — injection fuzz results, caps, path/permission behavior, denylist gate, `cargo audit`/`cargo deny` clean, adversarial stored-content test (memory containing prompt-injection text and FTS/JSON metacharacters round-trips verbatim, breaks nothing).
- Docs: README (contract, JSON schemas, exit codes, offline guarantee), `folder-chief-conventions.md` (recommended metadata conventions, when-to-save/ask guidance from `architecture.md` §3), `THIRD-PARTY.md` final.
- Tag v1.0.0 after final review.

**Acceptance criteria:** DoD §25 fully satisfied; every gate row has linked evidence.

**Final independent review:** a T1 model that did not implement v1 verifies DoD item-by-item and files the closing `result-review.md` entry; Lee accepts release.

**Likely failure modes:** macOS notarization latency with a 120MB Mach-O (start the signing setup at sprint start, not end); musl + `cc` cross pain for any C in the tree (there should be none beyond bundled SQLite — it builds under musl routinely).

---

## Dependency graph and gates

```
S1 ──G1(model decision: Lee)──▶ S2 ──▶ S3 ──▶ S4 ──▶ S6 ──▶ v1.0.0
                                        └──▶ S5 ──G2(if gates fail: Lee)──┘
```

Standing gates across all sprints: network-denylist CI check; stdout-single-JSON contract tests; no sprint merges without its `result-review.md` entry; `decisions.md` updated only for decisions Lee actually accepts.

## Buy/build/fork/steal actions checklist

- [ ] Open issue on rag-ferrite asking the author to add the declared-MIT LICENSE file (patterns-only until answered).
- [ ] Lift Satchel embed/chunker/FTS-schema/release.yml with MIT attribution (S2/S6).
- [ ] Lift sqlite-graphrag output-sink and migration patterns with MIT/Apache attribution (S2).
- [ ] RavenRustRAG: study-only notes for release matrix; verify no AGPL code is ever copied (S6 review item).

## Documentation requirements (rolling)

Every sprint updates: `context.md` (state + next step), `result-review.md` (evidence entry), `sprint-plan.md` progress column — per `AGENTS.md` discipline. `architecture.md` is amended (with a changelog line) whenever reality diverges; divergence without amendment is a review failure.
