# sqlite-mem Result Review

This file records completed, reviewed work and the evidence supporting it.

## Current Review State

Sprint 0 (research + architecture) executed 2026-08-31; outputs below are
submitted for Lee's review and ratification. No implementation exists.

### 2026-08-31 — Sprint S2: core storage and SAVE

- **Objective:** Repo scaffold, schema v1 + migrations, JSON output sink,
  full `save` verb, basic `info`, CI with network-denylist gate
  (project-plan.md S2).
- **Verified outcome:** ACCEPTED. Product crate at repo root (binary
  `sqlite-mem`, ~2.2k LoC src + ~0.6k tests). All S2 acceptance criteria
  verified INDEPENDENTLY by the supervisor, not taken from the worker's
  report: 73 tests green (unit incl. chunker proptests; integration against
  the real binary for exit codes, output discipline, migrations, save
  contract), fmt/clippy clean across all three feature sets, acceptance
  transcript reproduced in a clean directory against the real granite
  sidecar model (save → documented JSON, dedup idempotency, validation
  errors with exit 3 and typed envelope, 0700/0600 permissions,
  supersession transitions, FTS sync verified at S2 build time).
- **Defect found in supervision and fixed:** dedup silently dropped
  `--supersedes` (retire-intent lost on idempotent retries). §11.2
  amended; fix implemented with three new tests (retire-at-existing-id,
  second-retry idempotency, self-supersession no-op); fix re-verified live
  by the supervisor. Two worker judgment calls accepted and codified:
  `--if-new` duplicate → exit 3 `not_new`; missing `--db` parent → exit 5
  `db_path_unavailable`.
- **Candle pin decision (per D014, supervisor decision on evidence):**
  **candle 0.9.1 + tokenizers/fancy-regex.** Worker evaluation showed the
  ModernBERT module compiles unchanged and the tree drops all C deps;
  supervisor re-ran the full S1 parity harness against that exact
  configuration — PASS on all 100 texts (min cosine ≥ 0.999999) — then
  switched the product, re-verified 73 tests, clippy, a zero-hit
  denylist/onig tree scan, and a real-model save smoke. Fallback if 0.9.x
  maintenance decays: candle 0.11 + musl-tools (documented in
  architecture.md changelog).
- **Evidence:** test runs and transcripts in this session (supervisor-
  executed); CI workflow `.github/workflows/ci.yml`; THIRD-PARTY.md;
  parity output for candle 0.9.1 configuration (scratchpad
  `candle091_granite.json` vs committed `reference_granite.json` — PASS).
- **Deviations:** none unresolved.
- **Remaining risks or blockers:** CI workflow not yet exercised on GitHub
  (no remote); embed-model full-release build re-measured at S6.
- **Next authorized action:** Sprint S3 (ASK hybrid retrieval) per D015.

### 2026-08-31 — Sprint S1: embedding parity and packaging spike

- **Objective:** Prove Candle can run the candidate models offline with
  reference-grade fidelity inside the footprint budget (project-plan.md S1);
  close architecture.md §26.1.
- **Verified outcome:** ALL FIVE acceptance criteria PASS. Parity vs
  sentence-transformers (true-f32 reference): min cosine 1.000000000 at
  9 decimals across all 100 fixture texts for BOTH granite-small-r2 and
  bge-small-en-v1.5. Embedded-weights binary 104 MiB (gate ≤150MB); cold
  start 0.57s median (gate <1.5s); musl static build succeeds and matches
  gnu to the last ulp. Deviations/findings: (F1) stock Candle ModernBERT
  OOMs at 8192 tokens (16.3GB dense attention) — a 130-line derived
  memory-efficient module (1.42GB peak, ~24% faster, component-identical)
  is now a required product component; (F2) candle-core ≥0.10 pulls C
  oniguruma via tokenizers/onig — musl needs musl-tools or a candle 0.9.1
  pin; (F3) vector-level byte-determinism is unattainable across libms —
  contract restated as rounded-output byte-identity; (F4) 8192-token embed
  costs ~39s, confirming chunk-before-embed; granite ships bf16 (f16
  conversion lossless) and has no normalize module (product normalizes).
- **Evidence:** `spike/embed-parity/REPORT.md`; vector sets in
  `spike/embed-parity/out/` (candle_*.json, reference_*.json);
  `compare.py` output PASS/PASS; derived module
  `spike/embed-parity/rust/src/modernbert_mem.rs`; reproduction commands in
  REPORT.md. Executed by Opus 5 (Rust/Candle) and Sonnet (Python reference)
  subagents under Fable supervision; parity computed independently by the
  supervisor, not by either implementing agent.
- **Decisions requested or made:** G1 requested from Lee — recommend
  adopting granite-embedding-small-english-r2 (f16, embedded) with
  bge-small-en-v1.5 as tested fallback; and authorization of S2+.
  architecture.md §7/§9/§26 amended per the ratified divergence rule
  (changelog added).
- **Deviations:** modernbert_mem derived module (F1) — cross-checked
  against stock to 8.2e-8; determinism gate restatement (F3).
- **Remaining risks or blockers:** candle version pin decision (S2 setup);
  release builders need ≥10GB RAM; retrieval-quality question (§26.2)
  remains for S5.
- **Next authorized action:** Await Lee's G1 decision and S2+
  authorization.

### 2026-08-31 — Sprint 0: candidate research and proposed architecture

- **Objective:** Execute Sprint 0 under Lee's authorization: validate
  boundaries, research upstream candidates and the technology landscape,
  perform buy/build/fork/steal analysis, and produce `architecture.md` and
  `project-plan.md`.
- **Verified outcome:** Four candidate repos cloned and inspected at source
  level. Satchel: MIT, proves the exact target stack (Rust + rusqlite
  bundled + FTS5 + Candle + bge-small embedded via include_bytes, ~105MB
  shipped, 5-target CI) but daemon/UI-shaped with ~70% unwanted scope —
  reuse modules. rag-ferrite: MIT-intended but no LICENSE file, active; best
  sqlite-vec/RRF/benchmark reference — steal patterns. sqlite-graphrag:
  MIT/Apache; best JSON-contract, lifecycle-verb, and migration discipline —
  reuse patterns. RavenRustRAG: AGPL — study-only. Landscape survey found no
  existing tool combining bundled model + zero network + save/ask CLI +
  deterministic JSON: the niche is empty. Embedding-model survey identified
  granite-embedding-small-english-r2 (Apache-2.0, 47M, 384d, 8192 ctx, ~95MB
  f16) as best quality-per-MB in budget, bge-small-en-v1.5 (MIT) as de-risk
  fallback; EmbeddingGemma rejected on license. Two of three candidates
  independently found sqlite-vec unnecessary at memory scale, supporting
  BLOB brute-force for v1.
- **Evidence:** Subagent research reports (session of 2026-08-31); candidate
  clones inspected in session scratchpad; load-bearing claims (licenses,
  sizes, model data, sqlite-vec status) carry source URLs in the survey
  report and are reflected in `architecture.md` §4–§9. Note: web-research
  claims were verified against repo sources by the research agents, not
  independently re-verified; Sprint S1 re-verifies the load-bearing
  model/runtime claims empirically.
- **Decisions requested or made:** Lee accepted D009 (≤150MB quality-first),
  D010 (fully offline, bundled model), D011 (MIT/Apache permissive posture).
  Requested: ratification of `architecture.md` + `project-plan.md`; D004
  amendment (mechanical lifecycle verbs); later gates G1 (model) and G2
  (benchmark fallback) per `project-plan.md`.
- **Deviations:** None from product-definition boundaries. The D004
  interface question is flagged as an amendment request, not applied.
- **Remaining risks or blockers:** Candle × ModernBERT parity (S1 closes);
  retrieval quality of a 47M model on distilled memories (S5 closes);
  rag-ferrite license confirmation; Windows float determinism (S6 measures).
- **Next authorized action:** Ratified by Lee 2026-08-31 (D012, D013; D004
  amended). Execute Sprint S1 (embedding parity spike); S2–S6 await G1 and
  Lee's authorization.

## Review Entry Template

### YYYY-MM-DD — Result

- **Objective:**
- **Verified outcome:**
- **Evidence:**
- **Decisions requested or made:**
- **Deviations:**
- **Remaining risks or blockers:**
- **Next authorized action:**
