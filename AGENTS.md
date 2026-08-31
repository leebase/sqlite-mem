# Agent Guide: sqlite-mem

This repository uses AgentFlow: repository files are shared operational memory
for human and AI collaborators. Keep them accurate enough that a new session
can resume without reconstructing intent from chat history.

## Startup Protocol

Read these files in order before changing the project:

1. `AGENTS.md`
2. `product-definition.md`
3. `context.md`
4. `decisions.md`
5. `result-review.md`
6. `sprint-plan.md`

Confirm the current phase and authorization boundary before acting. Re-read the
relevant authority file before changing scope, architecture, or implementation.

## Current Authority

- `product-definition.md` is the current product authority.
- `decisions.md` contains only decisions actually made.
- Research findings and candidate techniques are not architecture decisions.
- `context.md` records current state and the next authorized step.
- `sprint-plan.md` defines planned work; a listed task is not automatically
  authorization to implement it.
- `result-review.md` records reviewed outcomes and evidence.

If files conflict, stop and surface the conflict. Do not silently promote a
proposal, example, research finding, or prior chat statement into a decision.

## Operating Boundaries

Until explicitly authorized:

- Do not implement sqlite-mem.
- Do not select an implementation language, embedding model/runtime, vector
  extension, database schema, protocol syntax, or packaging mechanism.
- Do not add services, daemons, containers, hosted dependencies, installers,
  provider configuration, or required language environments.
- Do not crawl or inspect arbitrary caller files on sqlite-mem's behalf.
- Do not mutate caller-owned source material.
- Do not modify Folder Chief or any other repository.

Keep sqlite-mem standalone, local-first, infrastructure-free, narrow in
authority, and useful to arbitrary AI harnesses.

## Work Discipline

Before work:

1. State the concrete objective and authority for it.
2. Inspect current repository state and preserve unrelated work.
3. Distinguish accepted decisions from open questions and research candidates.

After an authorized work unit:

1. Validate against measurable success criteria.
2. Update `context.md` with verified current state and next action.
3. Update `decisions.md` only for decisions actually accepted.
4. Record reviewed outcomes and evidence in `result-review.md`.
5. Update `sprint-plan.md` without overstating progress.

## Communication

Report verified outcomes, decisions, blockers, deviations, and exact next
steps. Do not claim an experiment proves portability, safety, retrieval quality,
or architecture unless its evidence supports that claim.
