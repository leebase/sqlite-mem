# Sprint Plan — sqlite-mem

## Sprint 0 — Product and Architecture Validation

**Status:** Executed 2026-08-31; outputs ratified by Lee 2026-08-31 (D012),
embedding model conditional on G1. Sprint S1 authorized (D013); S2–S6 not
yet authorized.

**Outcome:** `architecture.md` (proposed architecture, buy/build/fork/steal
analysis, benchmark strategy) and `project-plan.md` (executable sprint plan
S1–S6). Evidence in `result-review.md`.

## Executed Work

| Workstream | Status | Outcome |
|---|---|---|
| Validate product boundaries | Done | Boundaries confirmed; one amendment flagged (D004 lifecycle verbs, `architecture.md` §2) rather than silently applied |
| Identify implementation options | Done | Four candidates inspected at source; runtime/model/vector landscape surveyed; option analysis in `architecture.md` §4–§9 |
| Define a minimal technological proof | Done | Kernel proof + Sprint S1 embedding-parity spike specified (`architecture.md` §21, `project-plan.md` S1/S5) |
| Define measurable success criteria | Done | Gates: parity ≥0.999, recall@5 ≥0.85, MRR ≥0.7, hybrid ≥ each leg, cold start <1.5s, warm ask <250ms\@10K chunks, ≤150MB (`architecture.md` §25) |

## Forward Plan

Implementation sprints S1–S6 (spike → storage/SAVE → ASK → lifecycle →
benchmarks → packaging/security/release) are defined in `project-plan.md`,
which is the planning authority for implementation once ratified.

Decision gates returning to Lee:

- ~~Ratification gate~~ — met 2026-08-31 (D012, D004 amendment accepted).
- ~~G1~~ — closed 2026-08-31 (D014): granite-small-r2 f16 adopted, bge
  fallback retained, S1 amendments accepted, bounded chunking made
  explicit, Candle pin deferred to S2. S2–S6 authorized (D015).
- **G2** (during S5): only if benchmark gates fail on the primary model.
- **v1 acceptance** (after S6): Lee.

## Guardrails

- Nothing in `architecture.md` is an accepted decision until ratified;
  `decisions.md` remains the record of accepted decisions.
- No implementation before ratification and Sprint S1.
- Interface stays at two cognitive primitives plus the proposed mechanical
  lifecycle verbs; no retrieval-surface growth without a demonstrated need
  and an accepted decision.
- No services, installers, provider setup, network dependencies, or silent
  filesystem inspection — enforced by the CI network-denylist gate from S2.

## Definition of Ready for Implementation

Met when Lee ratifies the two documents: boundaries validated, minimal proof
specified, measurable criteria set, tradeoffs visible (`architecture.md` §4,
§26), and the relevant decisions approved.
