# sqlite-mem Result Review

This file records completed, reviewed work and the evidence supporting it.

## Current Review State

Sprint 0 (research + architecture) executed 2026-08-31; outputs below are
submitted for Lee's review and ratification. No implementation exists.

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
