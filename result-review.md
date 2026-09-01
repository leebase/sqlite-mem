# sqlite-mem Result Review

This file records completed, reviewed work and the evidence supporting it.

## Current Review State

Sprint 0 (research + architecture) executed 2026-08-31; outputs below are
submitted for Lee's review and ratification. No implementation exists.

### 2026-09-01 — v1.0.0 RELEASED (D017)

- **Objective:** Publish v1.0.0 per Lee's acceptance and authorization
  (D017).
- **Verified outcome:** RELEASED at
  https://github.com/leebase/sqlite-mem/releases/tag/v1.0.0 — full
  release (not prerelease), six assets (five platform zips +
  SHA256SUMS.txt), published by the rc6-proven workflow with all jobs
  green including the five-binary shared-DB byte-identity gate. One
  defect caught in the final published-artifact smoke and fixed before
  closing: the first v1.0.0 cut shipped binaries reporting version
  0.1.0 (Cargo version never bumped); version bumped to 1.0.0, release
  and tag re-cut, and the republished artifact verified — checksum OK,
  `--version` reports 1.0.0, empty-dir env -i kernel proof rank-1 HIT.
- **Evidence:** release run 33471802903 (all green); local verification
  of the published linux x86_64 asset in this session.
- **Remaining known items (unchanged, non-blocking per D017):** macOS
  binaries unsigned pending Lee's Apple Developer credentials;
  47M-embedder quality ceiling; §26.2 model ablation formally open.
- **Next authorized action:** none — v1 is complete and released.

### 2026-09-01 — CI evidence run: cross-platform gap CLOSED (rc1–rc6)

- **Objective (Lee's directive):** create/push the GitHub remote, run
  the full five-target CI matrix, inspect the resulting binaries rather
  than trusting green jobs, and close the cross-platform evidence gap
  before publishing downloads broadly. Also: make the repo public with
  open-source trimmings.
- **Verified outcome:** GAP CLOSED. Repo public at
  github.com/leebase/sqlite-mem (badges, CONTRIBUTING, SECURITY, code
  of conduct, Cargo metadata; MIT OR Apache-2.0 per D011). Six release
  candidates were needed — each earlier rc surfaced a real
  authored-but-never-executed defect: rc1 converter missing
  --models-dir; rc2 cross-Docker env passthrough (fixed by staging the
  model at build.rs's default path) + CI clippy needing model stubs;
  rc3 determinism harness structurally unpassable (per-platform DBs →
  differing ULIDs/timestamps; redesigned to ONE shared golden DB, which
  also tests cross-OS SQLite-file portability); rc4 proved the real
  claim (all platforms byte-identical after CR-strip) with the only
  divergence being Python's Windows newline translation in the harness;
  rc5 full green; rc6 extended execution to ALL FIVE artifacts.
- **Final evidence (run 33469882317, v1.0.0-rc6, all 14 jobs green):**
  model fetched fresh from the pinned HF revision with checksums
  verified (the self-derived pin held against an independent download);
  all five targets built; **all five shipped binaries executed** —
  linux x86_64, linux aarch64 (arm64 runner), macOS x86_64 (Rosetta),
  macOS arm64 (native Apple silicon), Windows x86_64 — each embedding
  the kernel query natively against one shared golden DB and producing
  **byte-identical 3,771-byte rounded JSON** (five-way diff, zero
  differences). Supervisor artifact inspection (not job-status trust):
  SHA256SUMS verified locally; file headers confirmed per target (ELF
  static/static-pie, Mach-O arm64/x86_64, PE32+); 102–105 MiB each
  (≤150MB gate); the released x86_64 artifact run locally in an empty
  dir under env -i: 0.585s cold ask, kernel rank-1, verify all-green.
- **Remaining known items:** macOS binaries are UNSIGNED (no Apple
  Developer secrets configured — downloaders will hit Gatekeeper;
  signing needs Lee's credentials); aarch64 binaries executed in CI but
  not on local hardware; rc releases are marked prerelease pending
  Lee's v1.0.0 acceptance.
- **Next authorized action:** Lee's v1 acceptance → tag v1.0.0 (the
  proven workflow will publish it).

### 2026-09-01 — Final independent DoD review — ACCEPT WITH RECORDED GAPS

- **Objective:** Independent item-by-item verification of the amended
  §25 Definition of Done by a fresh strong-model reviewer that
  implemented nothing (project-plan.md S6), including Lee's two
  mandated scrutiny items.
- **Verified outcome:** **ACCEPT WITH RECORDED GAPS — no blockers.**
  §25: items 2 (CLI contract), 6 (concurrency/recovery), 8 (docs) MET;
  3 (parity/determinism), 4 (benchmark gates), 7 (security/denylist)
  MET-WITH-GAPS all honestly recorded; 1 (five-platform CI) NOT MET and
  honestly recorded (no remote; only linux targets ever built). The
  reviewer independently reproduced the test suite, the 62-corpus
  benchmark (exact numbers), the empty-dir env -i release smoke, the
  exit-code table, and the denylist gate. Every architecture amendment
  traced to a recorded decision — no silent drift. Scrutiny item 1
  (4096 threshold reads as revisable measured policy): PASS. Scrutiny
  item 2 (recalibration evidence-driven, not goalpost-moving): PASS —
  verified at git level that the failing gate table was committed
  (b228177) BEFORE D016 existed and survives byte-unchanged; caveats
  disclosed: the shipped gate numbers came from the S5c leg-switch, not
  the tuning D016.3 literally contemplated (fully documented), and the
  "default ≥ pure modes" principle was miscited as a §24 invariant
  (fixed, see below).
- **Review findings D1–D9 (documentation-level, no code-behavior
  defects; no new security/correctness defects):** all pre-tag items
  applied by the supervisor and re-verified: D1 README documents the
  4096 scale-adaptive default; D2 §13 body synced to shipped retrieval
  behavior; D3 §25 item 5 synced to D016.2; D4 §24 miscitations fixed
  in src/ask.rs and bench/REPORT.md; D5 context.md rewritten (was
  dangerously stale — still claimed "no code exists"); D6 harness gate
  table now prints v1 gates with historical rows labeled; D7 citation
  fix; D9 README CI present-tense softened. D8 (model ablation never
  run) recorded as a standing gap here and in context.md.
- **Recorded gaps at acceptance:** four of five platforms unbuilt / CI
  never executed / no signing or checksums; cross-OS determinism
  linux-only; model ablation unrun (§26.2 formally open, decision-inert
  per D016); absolute quality = 47M embedder ceiling 0.8114/0.6697.
- **Decisions requested:** Lee's v1 acceptance. Tag v1.0.0 only after
  acceptance.

### 2026-09-01 — Sprint S6: packaging, security, release readiness

- **Objective:** Release workflow + local release verification, docs,
  independent security gate, fixes (project-plan.md S6).
- **Verified outcome:** ACCEPTED pending final independent DoD review.
  **Packaging (S6a):** release.yml authored for all five targets;
  locally proven: gnu and musl embed-model binaries 105.5/105.2 MiB
  (≤150MB gate), cold starts 553/635ms (<1.5s), gnu↔musl rounded-output
  determinism byte-identical, and the headline acceptance test — env -i
  in an empty directory, no configuration: save + cross-worded ask,
  rank-1 hit, one file created; self-containment by construction (the
  sidecar code path does not exist in release builds). README (full CLI
  contract, every example executed against the real binary),
  folder-chief-conventions.md, LICENSE-MIT/APACHE, THIRD-PARTY.md.
  **Security gate (S6b, independent Opus auditor per D015): first pass
  PASS-WITH-FINDINGS (2 HIGH, 4 MEDIUM, 5 LOW); after the S6c fix pass,
  targeted re-audit with fresh reproductions: SECURITY GATE PASS, no
  remaining blockers, no regressions.** Fixed and re-verified: F1
  unfalsifiable FTS-desync check (rank-1 integrity-check; 3 desync
  regression tests), F2 --db "" silent data loss (exit 2), F3 denylist
  dead patterns + build-dep blind spot (26/27 false-pass battery caught,
  0 false positives; cargo-audit CI job added), F4 unqualified temp-table
  DROP destroying user tables (temp.-qualified), F5 unbounded stdin
  (1GB pipe: 1,053MB → 6.5MB RSS), F6 sidecar env override live in
  embed-model builds (compile_error! guard; also CAUGHT AND FIXED
  release.yml missing --no-default-features — the packaging worker's
  claim was verified false by the fix worker), F10/F11/INFO-b, plus the
  re-audit's two LOWs (multibyte over-cap diagnostic; build.rs in the
  net-grep) fixed by the supervisor. Tests 177 → 194 green.
- **Evidence gaps recorded honestly:** macOS/Windows/aarch64-musl
  builds, signing/notarization, CI-side model fetch, and cross-OS
  determinism are authored but unexecuted (no remote/toolchains) — the
  first real CI run is the outstanding verification step. The HF model
  pin is self-derived from the S1-parity-tested weights, not a published
  manifest. RUSTSEC-2024-0436 (paste, unmaintained, transitive):
  advisory-only.
- **Deviations:** none unresolved.
- **Next authorized action:** final independent DoD review (fresh
  reviewer, per project-plan.md S6), then v1 acceptance by Lee.

### 2026-09-01 — Sprint S5b/S5c: retrieval tuning — S5 CLOSED under D016.3

- **Objective:** D016.1 tuning pass (DF token filtering + corpus-scaled
  lexical cap), preserve the 10K crossover, meet D016.2 latency gates.
- **Verified outcome:** ACCEPTED, with D016.3 recalibration applied.
  S5b implemented both mechanisms (plus a supervisor-approved DF floor
  and a rowid-materialization perf fix that brought worst-case retrieval
  from ~80ms to 31ms). Honest finding: NO configuration within the
  D016.1 bounds made small-corpus fusion beat semantic-only (best tuned
  0.597/0.517 vs semantic 0.811/0.670) — the lexical noise floor was
  mostly harmless, and filtering exposed harder content-word collisions.
  **Supervisor ruling (S5c):** the corpus-scaled cap taken to its
  measured limit — lexical leg activates only at ≥ 4096 allowed chunks;
  below that the default ranks purely semantically. Worker verified at
  the ranking level (byte-identical id lists) that the default now
  equals semantic at 62/holdout and equals the tuned hybrid at 10K
  (crossover preserved, margin +0.035). Supervisor independently re-ran
  the 62 benchmark and holdout: 0.8114/0.6697 main, 0.625/0.542
  holdout, default ≥ each pure mode everywhere; 177 tests green.
- **D016.3 applied (pre-authorized by Lee):** gates recalibrated to
  achieved evidence — default-mode recall@5 ≥ 0.80 / MRR ≥ 0.65 on the
  golden benchmark + default ≥ each pure mode at every measured scale.
  Reasoning recorded in the architecture changelog: the original
  0.85/0.70 was set pre-evidence; the achieved ceiling is the 47M
  embedder's on a 71%-zero-overlap adversarial dataset, not a fusion
  defect. Latency gates (D016.2) pass: retrieval-only 23–34ms at 10K,
  end-to-end ~530ms < 1s, cold start < 0.5s.
- **Evidence:** bench/REPORT.md §S5b/§S5c (appended, original failing
  section preserved); bench/results/*-S5b, *-S5c; supervisor re-runs
  this session.
- **Next authorized action:** Sprint S6 (packaging, security, release)
  per D015.

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
