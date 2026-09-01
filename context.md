# sqlite-mem Session Context

## Snapshot

| Attribute | Value |
|---|---|
| Project | sqlite-mem |
| Phase | v1 complete; final independent review done; awaiting Lee's v1 acceptance |
| Status | Security gate PASS; 194 tests green; review verdict ACCEPT WITH RECORDED GAPS |
| Product authority | `product-definition.md` |
| Architecture authority | `architecture.md` (ratified D012, amended via its changelog) |
| Decision record | `decisions.md` D001–D016 (Lee's decisions only) |
| Evidence trail | `result-review.md` (every sprint, with supervisor verification) |
| Last updated | 2026-09-01 |

## Current State

- The product is implemented and locally proven: a single Rust binary
  (`sqlite-mem`) with `save`, `ask`, `forget`/`--restore`/`--purge`,
  `reindex`, and `info`/`--verify`; granite-embedding-small-english-r2
  f16 embedded via `include_bytes!` (bge-small-en-v1.5 the validated
  fallback); one user-owned SQLite file; deterministic single-JSON
  stdout with typed exit codes; zero network capability in the
  dependency tree (CI-gated).
- Retrieval is scale-adaptive per the S5c ruling under D016: below
  4,096 stored chunks the default ranks purely semantically; at scale
  it runs DF-filtered, corpus-capped hybrid RRF. The threshold is an
  empirically calibrated, revisable policy (architecture.md changelog).
- Measured quality: recall@5 0.8114 / MRR 0.6697 on the 71%-zero-overlap
  adversarial golden benchmark — the v1 gates (≥0.80/≥0.65 per D016.3)
  pass; the default mode ≥ each pure mode at 62/1K/10K chunks.
- Release: linux gnu+musl embed-model binaries proven locally (105 MiB,
  cold start <0.7s, empty-dir `env -i` save/ask smoke passes). Sprint
  history and commits: S1 `de1c64c`, S2 `74e049e`, S3 `b2683cc`,
  S4 `8114111`, S5 evidence `b228177`, S5 close `7e0b427`,
  S6 `a024964`.
- Security: independent audit → fix pass → re-audit → **PASS** (no
  remaining blockers; 194 tests).
- Final independent DoD review (2026-09-01): **ACCEPT WITH RECORDED
  GAPS**; both of Lee's mandated scrutiny items pass (4096 threshold
  reads as revisable measured policy; gate recalibration trail is
  complete and honest, failure committed before authorization existed).
  Its pre-tag documentation fixes (D1–D7, D9) have been applied.

## Known Gaps (recorded, not blocking; see result-review.md)

- Four of five platform targets (macOS arm64/x64, Windows, aarch64
  musl) are authored in `release.yml` but have never been compiled —
  no GitHub remote, so CI has never executed. First real CI run is the
  outstanding verification step before any download link is promised.
- Cross-OS determinism verified linux gnu↔musl only.
- The granite-vs-bge model ablation was never run (architecture.md
  §26.2 formally open; D016 foreclosed the model route, so it no longer
  gates a decision).
- Absolute retrieval quality is the 47M embedder's measured ceiling.

## Authority Boundary

All sprints S1–S6 were authorized (D013, D015) and are closed with
evidence. Supervisor rulings (candle 0.9.1 pin, verify envelope,
lexical-activation threshold) are recorded in `result-review.md` and the
architecture changelog — deliberately not in `decisions.md`, which holds
only Lee's decisions.

## Next Authorized Step

Present v1 to Lee for acceptance. Tag v1.0.0 only after Lee accepts
(project-plan.md S6). After acceptance, the natural next steps are:
push to a GitHub remote and execute the first real CI run (closing the
platform evidence gap), then address the deferred-features list
(architecture.md §23) only on demonstrated need.
