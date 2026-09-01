# sqlite-mem Decisions

Only accepted product decisions belong here. Research findings, examples, and
candidate implementation techniques remain proposals until explicitly decided.

## Accepted Decisions

### D001 — Standalone product

sqlite-mem is a standalone product, separate from Folder Chief. Folder Chief
may consume it, but the product must remain generally useful to arbitrary AI
harnesses.

### D002 — Local file and transient process model

Persistent state lives in a user-owned local SQLite file. The sqlite-mem
process starts for an operation, reads or updates that file, returns
deterministic machine-readable output, and exits. It is not a server, daemon,
service, container, or autonomous agent.

### D003 — Portable, no-install distribution target

The target distribution is self-contained, no-install binaries for macOS,
Linux, and Windows. The precise language, runtime, model packaging, supported
CPU architectures, and build mechanism remain undecided.

### D004 — Two cognitive primitives plus minimal mechanical lifecycle verbs

*(Amended 2026-08-31 by Lee.)* The interface is organized around two cognitive
primitives — `SAVE THIS CONTENT` and `ANSWER THIS QUESTION` — plus the
mechanical lifecycle verbs `forget`, `reindex`, and `info`. These verbs add no
cognition and no retrieval surface; the retrieval surface does not grow
without a demonstrated need and an accepted decision. Command syntax and
protocol schemas are defined in `architecture.md` (ratified, see D012).

### D005 — Calling AI owns judgment; sqlite-mem owns mechanics

The calling AI decides what to inspect, what matters, what to save, and what
metadata to supply. sqlite-mem does not crawl whole folders. It owns safe
storage and transactions, indexing and retrieval mechanics, deterministic
local embedding generation, hybrid ranking, metadata persistence/filtering,
deterministic output, and appropriate provenance/system metadata.

### D006 — No user-configured AI or embedding infrastructure

The user must not need to configure an LLM, provider, API key, Ollama, LM
Studio, embedding server, required cloud service, or required Python
environment. sqlite-mem owns the stable deterministic local encoding required
by its memory format; the exact implementation remains undecided.

### D007 — Metadata is configurable and first-class

Caller metadata is configurable at ingestion and retrieval. The design must
distinguish caller-supplied metadata, sqlite-mem system/provenance metadata,
and retrieval/ranking metadata without imposing a Folder-Chief-specific
ontology.

### D008 — Narrow authority and caller-source safety

sqlite-mem must remain operationally simple, local-first, user-owned, and
narrow in authority. It must not silently inspect arbitrary files or mutate
caller-owned source material.

### D009 — Footprint budget: quality-first up to ~150MB

Accepted by Lee on 2026-08-31. The total footprint of executable plus embedding
model may be up to ~150MB, with retrieval quality prioritized within that
budget over minimal size.

### D010 — Fully offline; model bundled

Accepted by Lee on 2026-08-31. sqlite-mem must be fully offline from the first
invocation. The embedding model ships inside (or immediately beside) the
binary. No first-run download, no network access ever.

### D011 — Permissive licensing posture

Accepted by Lee on 2026-08-31. sqlite-mem is licensed MIT OR Apache-2.0. Only
permissively licensed code and model weights may be reused or forked; copyleft
projects are study-only ("steal patterns," never code).

### D012 — Architecture and project plan ratified

Accepted by Lee on 2026-08-31. `architecture.md` and `project-plan.md` are
ratified: Rust, Candle runtime, FTS5 + BLOB brute-force vectors, RRF k=60,
the SAVE/ASK CLI contract, schema v1, and the safety/packaging strategy are
accepted architecture. **Exception:** the embedding model choice
(granite-embedding-small-english-r2, fallback bge-small-en-v1.5) remains
conditional on gate G1 — the Sprint S1 parity spike — and returns to Lee.

### D013 — Sprint S1 only is authorized

Accepted by Lee on 2026-08-31. Sprint S1 (embedding parity/packaging spike)
is authorized for execution. Sprints S2–S6 are NOT authorized; they require
G1 to close and Lee's go-ahead, so that Candle/ModernBERT feasibility and
real packaging/performance numbers are proven before the plan gains
expensive momentum.

### D014 — G1: embedding model adopted; bounded chunking is the product contract

Accepted by Lee on 2026-08-31, closing gate G1 on S1 evidence
(`spike/embed-parity/REPORT.md`).

- v1 embedded model: **granite-embedding-small-english-r2, f16, embedded
  in the binary**. Validated fallback: **bge-small-en-v1.5** (retained,
  parity-tested, same 384 dims).
- The S1 architecture amendments are accepted: the tokenizers/oniguruma
  C-dependency reality (§7), rounded-output determinism in place of
  raw-float byte identity (§9/§21), mandatory pre-embedding chunking, and
  the memory-efficient ModernBERT module as a **required product
  component**.
- **Explicit:** the model's 8192-token context is a model capability, not
  a product promise of single-chunk embedding. sqlite-mem enforces bounded
  chunk sizes (≤1024 tokens, 64-token overlap) because transient-CLI
  economics outrank theoretical max context.
- The Candle version choice (0.9.1 pure-Rust vs ≥0.10 + musl-tools) is
  deliberately **not** part of G1: it is an S2 implementation decision, to
  be made on API stability, maintenance risk, and whether avoiding
  oniguruma materially simplifies release engineering.

### D015 — Sprints S2–S6 authorized under the agreed staffing model

Accepted by Lee on 2026-08-31. Implementation sprints S2–S6 are authorized
per `project-plan.md`, staffed as: Sonnet 5 implementation workers,
Haiku 4.5 for T3 mechanical tasks, Opus-class-or-better supervision and
judgment work (S5 dataset design, S6 security review, acceptance reviews).
Supervisors review evidence, not worker summaries; workers stop and
escalate on any spec deviation. Sprint S1 is committed as a gate-closing
commit before S2 work begins, so G1 has an immutable authority point.
Remaining Lee gate: G2 only if S5 benchmark gates fail on the primary
model; v1 release acceptance.

### D016 — G2: retrieval-gate response

Accepted by Lee on 2026-08-31, on S5 benchmark evidence (result-review.md).

1. **S5b tuning pass authorized:** fix the verified lexical-leg noise
   (document-frequency-based query-token filtering and a corpus-scaled
   lexical candidate cap), re-measured at 62 / 1K / 10K chunks plus the
   blind holdout. The 10K-scale hybrid ≥ semantic crossover must be
   preserved.
2. **Warm-latency gate restated:** end-to-end warm ask < 1 s at 10K
   chunks AND retrieval-only (lexical path) < 50 ms; cold start < 1.5 s
   unchanged. The prior 250 ms gate conflated flat per-invocation model
   load (~500 ms) with retrieval (~3 ms).
3. **Pre-authorized fallback:** if S5b still misses recall@5 ≥ 0.85 /
   MRR ≥ 0.70, recalibrate the gates to the best-achieved tuned numbers
   (recorded with evidence) and proceed to S6 without a further
   check-in. The model route (arctic-m int8 via ort) is NOT authorized.

### D017 — v1.0.0 accepted and release authorized

Accepted by Lee on 2026-09-01. sqlite-mem v1.0.0 is ACCEPTED:
cross-platform build, execution, SQLite-file portability, and
deterministic-output evidence are complete across Linux x86_64, Linux
arm64, macOS x86_64, macOS arm64, and Windows x86_64 (run v1.0.0-rc6;
result-review.md 2026-09-01). The unsigned-macOS limitation is
documented and non-blocking for v1 correctness. Creation and
publication of the v1.0.0 release is authorized.

## Explicit Non-Decisions

- Candle version pin (0.9.1 vs ≥0.10) — S2 implementation decision, per
  D014.
- G2 fallback model swap — only if S5 gates fail.
