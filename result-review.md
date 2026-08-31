# sqlite-mem Result Review

This file records completed, reviewed work and the evidence supporting it.

## Current Review State

Sprint 0 (research + architecture) executed 2026-08-31; outputs below are
submitted for Lee's review and ratification. No implementation exists.

### 2026-08-31 — Sprint S5: benchmark suite — GATES FAILED, G2 ESCALATED

- **Objective:** Golden dataset, harness, ablations, ops metrics, gate
  verdicts (project-plan.md S5). Executed at supervisor tier (dataset
  judgment work) per D015.
- **Verified outcome:** Deliverables complete and honest; **retrieval
  gates FAIL** — S5 is NOT accepted; G2 escalated to Lee. Dataset: 62
  memories / 5 fictional projects, 38 queries (71% share zero content
  words with gold; 21% authored blind after harness freeze — holdout
  reproduces the finding). Supervisor independently re-ran the full
  benchmark (numbers reproduce exactly) and the metric selftest.
- **Gate results:** hybrid recall@5 0.636 (gate 0.85) FAIL; hybrid MRR
  0.497 (gate 0.70) FAIL; hybrid ≥ lexical PASS; hybrid ≥ semantic FAIL
  (semantic-only: 0.811 / 0.670); cold start 0.45–0.48s PASS; warm ask
  at 10K chunks 532ms (gate 250ms) FAIL; binary-size gate deferred to
  the S6 embed-model build.
- **Root cause, verified by supervisor probe:** build_fts5_query
  OR-joins every token including stopwords — "the and of a to" lexically
  matches 62/62 memories; the kernel question matches 38/62. Rank-based
  RRF then feeds that noise into fusion. Effect inverts with scale
  (hybrid beats semantic at 10K chunks): fusion design is sound at
  scale, mis-tuned for small corpora. Latency: retrieval itself is
  ~3ms; ~500ms is per-invocation model load+query-embed, flat with
  corpus size — the 250ms gate conflates the two and is unachievable
  for any transient CLI that embeds the query.
- **Also:** no binary defects in ~11,000 saves / ~1,200 asks; token
  economy 5.2× at 62 memories → ~763× at 10K. §12's illustrative ranks
  noted as unreachable at corpus scale (doc defect). S4 close commit
  accidentally swept in-progress bench files (.pyc now untracked).
- **Decisions requested (G2, Lee):** see context.md — (1) authorize
  bounded retrieval-tuning iteration S5b; (2) approve warm-latency gate
  restatement; (3) hold recall gates pending S5b vs. model-route
  decision.
- **Next authorized action:** await Lee's G2 decision. S6 blocked on it.

### 2026-08-31 — Sprint S4: lifecycle and integrity

- **Objective:** forget/--restore/--purge, reindex, info --verify,
  embedder-mismatch refusal, concurrency stress (project-plan.md S4).
- **Verified outcome:** ACCEPTED. Tests 128 → 163 green (+1 ignored
  stress test, 10/10 consecutive green runs of 8 writers × 100 saves + 4
  askers, zero caller-visible busy errors, verify clean after every run —
  no flock layer needed). Supervisor independently reproduced: full
  forget→restore→purge cycle (purge destructive-labeled, FTS finds
  nothing after), exit 4 on unknown id, the mismatch matrix (save and
  hybrid/semantic ask exit 6 with reindex hint; lexical ask/info/forget
  still work), reindex with pre-.bak restoring hybrid function, verify
  exit 0 healthy / exit 7 on tampered content.
- **Supervision ruling:** verify's failure envelope changed from ok:true+
  passed:false to the uniform contract (every non-zero exit ⇒ ok:false;
  error.code integrity_failed + top-level checks detail) — §18 amended,
  fix implemented and re-verified live. Other judgment calls accepted:
  restore preserves superseded status (superseded_by as marker); purge
  nulls dangling superseded_by (rare restore-after-purge corner noted as
  known mild edge); unconditional mismatch gate on save; reindex on a
  missing path consistent with ask's auto-create (document in S6).
- **Evidence:** supervisor-run transcripts this session; test suite.
- **Next authorized action:** await S5 benchmark verdicts; then S6.

### 2026-08-31 — Sprint S3: ASK hybrid retrieval

- **Objective:** Full `ask` verb per architecture.md §12–13
  (project-plan.md S3).
- **Verified outcome:** ACCEPTED. New modules vector.rs (VectorIndex trait
  + brute-force cosine), rank.rs (RRF k=60), filter.rs (=, !=, =* resolver
  via temp allowed-ID table shared by both legs), ask.rs (pipeline +
  exact §12 JSON). Tests 73 → 128, all green; fmt/clippy clean across
  three feature sets. Supervisor INDEPENDENTLY reproduced: kernel proof
  with a fresh decoy set (Mastra memory rank 1 in hybrid for "Why didn't
  we use that agent framework?", correct per-leg ranks/provenance),
  metadata-filtered ask, determinism (byte-identical minus elapsed_ms),
  lexical-mode empty results, exit codes 3 (empty query) and 2 (k out of
  range). Sanitizer fuzz + hand-computed RRF fixtures + collapse-before-
  truncate tests present as contracted.
- **Worker-caught spec defect:** the §12 example score (0.03252) did not
  match the stated RRF formula (correct: 0.03202) — example corrected;
  formula unchanged and verified by fixtures. Six judgment calls accepted
  and codified in the changelog (empty query exit 3; --k 1–50; malformed
  --where exit 2; content = full memory; candidates = pre-collapse union).
  Stale S2 doc comment (candle 0.11) fixed by supervisor.
- **Evidence:** supervisor-run transcripts this session; commit history.
- **Deviations:** none unresolved.
- **Remaining risks:** retrieval quality at realistic corpus scale is
  measured in S5 (kernel scenario passing at 7 memories is necessary, not
  sufficient).
- **Next authorized action:** Sprint S4 (lifecycle/integrity) and S5
  (benchmarks) per D015; S5 dataset design at supervisor tier.

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
