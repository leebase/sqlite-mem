# sqlite-mem Session Context

## Snapshot

| Attribute | Value |
|---|---|
| Project | sqlite-mem |
| Phase | S5 gates FAILED — G2 escalated to Lee; S6 blocked |
| Status | Binary healthy (163 tests, 0 defects in 11K ops); retrieval mis-tuned |
| Product authority | `product-definition.md` |
| Architecture authority | `architecture.md` (ratified; G1 closed: granite f16) |
| Last updated | 2026-08-31 |

## Current State

- Sprint 0 (research narrowing + architecture) was executed on 2026-08-31
  under Lee's explicit authorization.
- Four upstream candidates (Satchel, rag-ferrite, sqlite-graphrag,
  RavenRustRAG) were cloned and inspected at source level; a 2026 landscape
  survey of embedding runtimes/models/SQLite vector search was completed.
  Findings are summarized in `result-review.md` and `architecture.md` §4.
- Verdict: build fresh, reuse permissive modules (Satchel embed/chunker/
  FTS schema, sqlite-graphrag output/migration patterns), steal patterns
  elsewhere. The niche is verified empty — nothing existing bundles a model,
  guarantees zero network, and offers save/ask with deterministic JSON.
- Lee accepted three product decisions (D009 footprint ≤150MB quality-first,
  D010 fully offline bundled model, D011 permissive MIT/Apache licensing).
- `architecture.md` and `project-plan.md` are written and self-consistent
  with the product definition, with one flagged amendment request (D004
  lifecycle verbs, `architecture.md` §2).
- No code exists; no implementation decision is accepted until Lee ratifies
  the two documents.

## Authority Boundary

`product-definition.md` remains product authority. `architecture.md` and
`project-plan.md` were ratified by Lee on 2026-08-31 (D012), with the
embedding model conditional on gate G1. D004 was amended to admit the
mechanical lifecycle verbs (`forget`, `reindex`, `info`). Only Sprint S1 is
authorized (D013); S2–S6 require G1 closure and Lee's explicit go-ahead.

## Next Authorized Step

G1 closed by Lee 2026-08-31 (D014): granite-small-r2 f16 embedded, bge
fallback, bounded chunking (≤1024 tokens) as product contract, S1
architecture amendments accepted. Sprints S2–S6 authorized (D015) under
the agreed staffing model. S1 committed as a gate-closing commit.

**Sprint S2 is ACCEPTED and committed** (see result-review.md): scaffold,
schema v1 + migrations, output sink, full `save`, basic `info`, CI with
denylist gate; one supervision-caught defect (dedup dropped --supersedes)
fixed and re-verified; candle pinned 0.9.1 + fancy-regex on parity
evidence (pure-Rust tree). **Sprint S3 is ACCEPTED and committed**: full
`ask` (hybrid FTS5+vector, RRF k=60, metadata filters, deterministic
JSON), 128 tests green, kernel proof reproduced independently by the
supervisor (Mastra memory rank 1 cross-worded). **Sprint S4 is ACCEPTED
and committed**: forget/restore/purge, reindex with pre-backup, info
--verify (uniform ok:false on exit 7 per amended §18), embedder-mismatch
refusal matrix, 10/10 stress runs with no flock needed; 163 tests green.
**S5 gates FAILED — G2 is with Lee** (see result-review.md): hybrid
recall@5 0.636 vs 0.85, MRR 0.497 vs 0.70, semantic-only beats hybrid at
small scale (stopword noise in the lexical leg, verified by probe;
inverts at 10K chunks), warm-ask 532ms vs 250ms gate (which conflates
~500ms flat model-load with ~3ms retrieval). Decision request to Lee:
(1) authorize S5b bounded retrieval-tuning (document-frequency token
filtering / corpus-scaled lexical cap, re-measured at 62/1K/10K +
holdout); (2) restate the warm-latency gate to separate model-load from
retrieval; (3) hold recall gates pending S5b, with the G2 model route
(arctic-m int8 via ort) as the fallback if tuning cannot close the gap.
S6 does not start until G2 resolves.

## Open Questions

- rag-ferrite missing LICENSE file (issue to be opened; patterns-only until
  resolved).
- Windows rounded-output determinism (verified in Sprint S6).
- Retrieval quality of the 47M model on distilled-memory text
  (architecture.md §26.2; closed by S5 ablation, G2 if it fails).
