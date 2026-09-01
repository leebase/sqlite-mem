# sqlite-mem Session Context

## Snapshot

| Attribute | Value |
|---|---|
| Project | sqlite-mem |
| Phase | S6 complete; final independent DoD review pending |
| Status | Security gate PASS; release proven locally; 194 tests green |
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
**S5 is CLOSED under D016** (see result-review.md 2026-09-01): DF
filtering + corpus-scaled lexical cap implemented; tuning could not beat
semantic at small scale, so per the supervisor ruling the cap goes to
zero below 4096 chunks (default = semantic small, tuned hybrid at
scale; crossover preserved at 10K). Gates recalibrated per D016.3 to
achieved evidence: recall@5 >= 0.80 / MRR >= 0.65 + default >= each
pure mode at every scale (measured 0.8114/0.6697). Latency gates pass.
**Sprint S6 is complete** (see result-review.md 2026-09-01): release
workflow authored, linux gnu+musl embed-model binaries proven locally
(105 MiB, empty-dir env -i save/ask smoke passes — self-contained by
construction), docs written and executed against the real binary, and
the independent security gate is PASS after a fix pass (11 findings
fixed and re-verified; tests 177 → 194). Remaining: final independent
DoD review, then v1 acceptance by Lee. Outstanding evidence gap: the
macOS/Windows/aarch64 release paths need one real CI run once a GitHub
remote exists.

## Open Questions

- rag-ferrite missing LICENSE file (issue to be opened; patterns-only until
  resolved).
- Windows rounded-output determinism (verified in Sprint S6).
- Retrieval quality of the 47M model on distilled-memory text
  (architecture.md §26.2; closed by S5 ablation, G2 if it fails).
