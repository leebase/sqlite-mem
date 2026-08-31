# sqlite-mem Session Context

## Snapshot

| Attribute | Value |
|---|---|
| Project | sqlite-mem |
| Phase | G1 closed (D014); Sprints S2–S6 authorized (D015); S2 in progress |
| Status | Model: granite-small-r2 f16 embedded (bge fallback); S1 committed |
| Product authority | `product-definition.md` |
| Architecture authority | `architecture.md` (ratified; model conditional on G1) |
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

**Sprint S2 (core storage + SAVE) is in progress** per `project-plan.md`:
Sonnet worker implementing scaffold, migrations, output sink, `save`,
basic `info`, and CI gates under supervision. Acceptance review against
S2 criteria before S3 begins.

## Open Questions

- Candle pin: 0.9.1 (pure Rust) vs ≥0.10 + musl-tools — an S2
  implementation decision (D014); worker evaluates, supervisor decides.
- rag-ferrite missing LICENSE file (issue to be opened; patterns-only until
  resolved).
- Windows rounded-output determinism (verified in Sprint S6).
- Retrieval quality of the 47M model on distilled-memory text
  (architecture.md §26.2; closed by S5 ablation, G2 if it fails).
