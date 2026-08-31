# Sprint Plan — sqlite-mem

## Sprint 0 — Product and Architecture Validation

**Status:** Planned; not started

**Authorization:** Planning artifact only; do not execute without explicit
authorization

**Goal:** Narrow the product and architecture space enough to authorize a
minimal technological proof without prematurely selecting a full architecture.

## Planned Work

| Workstream | Status | Intended outcome |
|---|---|---|
| Validate product boundaries | Not started | Confirm the two-primitives model, authority limits, local/file/process model, and non-goals |
| Identify implementation options | Not started | Compare viable language, SQLite, embedding, vector, packaging, portability, licensing, and concurrency options |
| Define a minimal technological proof | Not started | Specify the smallest experiment that tests cross-wording retrieval with a transient executable and one local SQLite file |
| Define measurable success criteria | Not started | Establish objective retrieval, determinism, safety, portability, startup, size, and operational-complexity criteria |

## Sprint 0 Deliverables

- Product-boundary validation with contradictions or ambiguities surfaced
- Option matrix that separates evidence, tradeoffs, assumptions, and decisions
- Minimal proof specification with controlled inputs and expected outputs
- Measurable acceptance criteria and explicit non-claims
- Decision requests for Lee where alternatives materially change the product

## Guardrails

- Do not implement the proof or product during project initialization.
- Do not treat research suggestions as accepted architecture.
- Do not expand the interface beyond the two conceptual primitives without a
  demonstrated need and explicit decision.
- Do not introduce services, installation requirements, provider setup, or
  silent filesystem inspection.

## Definition of Ready for Implementation

Sprint 0 may recommend implementation only when product boundaries are
validated, a minimal proof is specified, measurable criteria exist, material
tradeoffs are visible, and Lee has approved the relevant decisions.
