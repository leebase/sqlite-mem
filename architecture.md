# sqlite-mem Architecture

**Status:** Ratified by Lee 2026-08-31 (D012), amended by the dated changelog entries at the bottom (each traceable to a recorded decision or supervisor ruling)

**Date:** 2026-08-31

**Author:** Solutions architecture pass (Sprint 0), based on direct inspection of Satchel, rag-ferrite, sqlite-graphrag, and RavenRustRAG source, plus a 2026 survey of embedding runtimes, models, and SQLite vector search.

---

## 1. Executive architectural summary

sqlite-mem is a single, fully offline, transient Rust executable that gives any AI harness durable, retrievable memory in one user-owned SQLite file. It exposes two cognitive primitives — `save` and `ask` — plus three mechanical lifecycle verbs (`forget`, `reindex`, `info`). It bundles its own embedding model inside the binary (Candle runtime, granite-embedding-small-english-r2 at f16, ~120MB total footprint), so no network, provider, key, daemon, or Python environment ever exists. Retrieval is hybrid: FTS5 BM25 + brute-force cosine over BLOB-stored vectors, fused with Reciprocal Rank Fusion (k=60), filtered by caller metadata, returned as deterministic JSON on stdout with stable ordering and typed exit codes.

**Buy/build/fork/steal verdict: BUILD a fresh codebase, REUSING permissively-licensed modules from Satchel and sqlite-graphrag, and STEALING patterns from rag-ferrite and RavenRustRAG.** No existing project is adoptable or forkable — every candidate fails the bundled-offline-embedding requirement, which is sqlite-mem's core differentiator. The niche is verified empty as of 2026-08-31.

**Why sqlite-mem should exist:** the landscape scan found nothing that combines (a) model bundled in the binary, (b) zero network ever, (c) plain save/ask CLI with deterministic JSON, (d) one SQLite file. The closest projects (remem, opencode-memory, Satchel) each require model downloads, embedding servers, or are daemon/MCP-shaped. Satchel independently proves the exact stack fits the footprint budget and cross-compiles to all five targets (~105MB shipped binaries).

## 2. Product boundary

Unchanged from `product-definition.md`. sqlite-mem answers exactly one question: *"What have I been asked to remember that is relevant now?"* It never crawls folders, never inspects files, never mutates caller sources, never runs as a service, never becomes an agent, and never opens a network connection.

**One recommended amendment to `product-definition.md` (flagged, not silently applied):** D004 says "two conceptual primitives." The architecture keeps two *cognitive* primitives but requires three *mechanical* verbs that fall under the mechanics sqlite-mem already owns per D005 ("safe transactions, schema/version compatibility, integrity/recovery"):

- `forget` — deletion/garbage control (a memory tool without deletion becomes a junk drawer)
- `reindex` — re-embedding after an embedding-model upgrade (version compatibility)
- `info` — database/version/integrity inspection (recovery, debuggability)

Recommendation: amend D004 to "two cognitive primitives plus minimal mechanical lifecycle verbs; no retrieval-surface expansion." These verbs add no cognition and no RAG surface.

## 3. Folder Chief integration model

Folder Chief works without sqlite-mem; sqlite-mem is an optional capability dropped into a Chief folder (or anywhere). Integration is a documentation convention, not code coupling:

- **The SQLite file is an index, not an authority.** Markdown files the user owns remain the source of truth. Every saved memory should carry `source` provenance (a relative file path, plus optionally a heading/anchor) so the AI can follow retrieved memory back to authoritative Markdown. Folder Chief's rule "current truth outranks stale historical claims" is enforced by the *caller* using returned `created_at`, `status`, and metadata — sqlite-mem reports freshness and supersession; it does not adjudicate authority.
- **When to SAVE:** at the moments Folder Chief already treats as memory-worthy — a decision accepted, a constraint discovered, a precedent established, a result reviewed, a preference stated. Save the *distilled statement* (1–10 sentences), not raw file dumps; the files already exist on disk. Attach Folder Chief's own conventions as metadata (e.g. `project`, `kind=decision`, `authority=accepted`, `status=current`, `source=decisions.md#D007`).
- **What NOT to save:** file contents wholesale, transcripts, anything derivable by opening the referenced file, or ephemeral working state. The anti-junk-drawer mechanism is threefold: caller discipline (save distillations), `--supersedes` at save time (new truth retires old truth), and `forget`.
- **When to ASK:** before substantial work (context assembly), when the AI suspects precedent ("have we hit this before?"), and when it doesn't know the filename or wording — the exact case where recursive tree exploration burns tokens. ASK returns the few most relevant distilled memories plus provenance, so the AI opens 1–2 authoritative files instead of exploring a tree.
- **ASK returns evidence, never answers.** There is no LLM in the binary; the calling AI performs synthesis. This also keeps output deterministic.
- **Cross-harness inheritance** works for free: any harness that can run a CLI and read JSON inherits memory written by any other, because embeddings are generated by sqlite-mem itself (D006), not by the calling harness.

## 4. Buy/build/fork/steal analysis

All four candidates were cloned and inspected at source level (clones retained in the session scratchpad; findings recorded in `result-review.md`).

| Candidate | License | Verdict | Reason |
|---|---|---|---|
| Satchel (virgilvox/satchel) | MIT (code); MIT/Apache models | **REUSE modules** | Proves the exact target stack (Rust+rusqlite bundled+FTS5+Candle+bge-small via `include_bytes!`, ~105MB, 5-target CI). But daemon/UI-shaped, no JSON-stdout CLI, no arbitrary metadata, ~70% unwanted scope, dormant solo project. |
| rag-ferrite (lelabdev) | MIT-intended (**no LICENSE file** — confirm before verbatim reuse) | **STEAL patterns** | Best sqlite-vec/RRF/benchmark reference; active. External embeddings; server scope; metadata is a JSON `LIKE`. |
| sqlite-graphrag (danilo-aguiar-br) | Apache-2.0 OR MIT | **REUSE patterns/modules** | Best protocol discipline: enforced JSON-envelope stdout, `remember/recall/forget/purge` lifecycle, refinery migrations + pre-migration backup, flock concurrency, exit-code contract tests. Embeddings need OpenRouter; graph layer is dead weight. |
| RavenRustRAG (egkristi) | **AGPL-3.0** | **REJECT code; study only** | Copyleft. Study its release matrix (musl static, packaging breadth) and its `ort` local-ONNX feature design. |
| remem, opencode-memory, sqlite-memory, Chroma, Letta, memory-MCP servers | various | **REJECT** | Each requires model downloads, embedding servers, Python, or is a library/extension rather than a save/ask CLI. |

**Specific reuse list (all MIT/Apache, lift with attribution):**

- Satchel `src/embed/mod.rs`: Candle BERT loader, `include_bytes!`/disk dual path, CLS-vs-mean pooling selection, 512-token truncation repair, L2 normalization, `Fixed`/`Unavailable` test embedders behind a feature flag.
- Satchel `src/rag/mod.rs`: FTS5 external-content schema + sync triggers + backfill repair, `build_fts5_query` sanitizer (quotes tokens, injection-safe), `embedding_to_blob`/cosine helpers, RRF skeleton.
- Satchel `src/ingest/mod.rs::chunk_text` + its proptest properties (no-word-loss chunking, char-offset tracking).
- Satchel `.github/workflows/release.yml`: 5-target matrix, model-in-CI, `--features embed-model`.
- sqlite-graphrag `src/output/`: single JSON sink for stdout, stderr-only logging, test-gated "no stray println" discipline; refinery migration layout with `PRAGMA user_version` sync and pre-migration `.bak` backup.

**Specific steal list (patterns, re-implemented):**

- rag-ferrite: resolve metadata filter to an allowed-ID set once, share it across both retrieval legs; `candidate_k = top_k × 4` when filtered; golden-dataset benchmark computing recall@k / MRR / nDCG with unit-tested metric math.
- sqlite-graphrag: soft-delete `forget` with `deleted_at` + hard `purge`; V013 lesson — dropping sqlite-vec for BLOB brute-force at memory scale.
- Satchel: untruncated dense leg (rank the whole corpus so post-filter recall isn't lost); neighbor-chunk context expansion (deferred, see §22).
- RavenRustRAG: musl static targets; per-platform packaging breadth (ideas only — AGPL).

## 5. Language: Rust

Every credible candidate and every needed component is Rust-native: Candle, HF `tokenizers`, `rusqlite` bundled (FTS5 on), the sqlite-vec crate if ever needed. Go has no embedded-inference story without cgo (forfeiting its portability advantage); Zig lacks tokenizer/safetensors/SQLite ecosystem. Rust cross-compiles to all five targets with pure-Rust dependencies. **Decision: Rust, edition 2021+, stable toolchain.**

## 6. SQLite integration

- `rusqlite` with `features = ["bundled"]` — compiles SQLite in, FTS5 enabled, no system SQLite dependency, identical SQLite version on every platform.
- One database file, standard SQLite format (openable by any sqlite3 tool — supports "recoverable with ordinary filesystem tools").
- On open: `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;` (Satchel omitted busy_timeout — a verified defect class we avoid).
- All writes in explicit `IMMEDIATE` transactions; one transaction per `save` (memory + chunks + FTS + embeddings are atomic).

## 7. Embedding runtime and model

**Runtime: Candle** (Apache-2.0/MIT) + HF `tokenizers` (Apache-2.0). *(Amended per S1:)* Candle ≥ 0.10 is not fully pure-Rust — it unconditionally pulls `tokenizers` with the `onig` feature (`onig_sys`, C oniguruma), so musl builds require `musl-tools` in CI (verified working) or pinning `candle-core` 0.9.1 (no `tokenizers` dep; identical ModernBERT path) — S2 setup decides the pin. Additionally, stock Candle ModernBERT materializes dense 8192² attention (16 GB peak, OOMs); the S1-derived memory-efficient module (`spike/embed-parity/rust/src/modernbert_mem.rs`, 1.42 GB peak, ~24% faster, component-level-identical output) is a required part of the product embed module. Candle is ~14× slower than ONNX Runtime on CPU, which is irrelevant for embedding one query per invocation (tens of ms). `ort` (ONNX Runtime) is the designated fallback if quality benchmarking demands an int8 ONNX model; its per-target static-build pipeline is the cost.

**Model (primary): `ibm-granite/granite-embedding-small-english-r2`** — 47M params, 384 dims, 8192-token context, Apache-2.0, ModernBERT architecture, best retrieval quality-per-MB inside the budget. Bundled as **f16 safetensors ≈ 95MB** → total binary ≈ 110–120MB, inside the 150MB budget.

**Model (de-risk fallback): `BAAI/bge-small-en-v1.5`** — 33M, 384 dims, 512-token context, MIT, plain BERT. Satchel already runs it under Candle in production shape; f16 ≈ 66MB. If Candle's ModernBERT path can't reach embedding parity with the Python reference (§Spike S1), we ship bge-small with zero schema change (same 384 dims).

**Rejected models:** EmbeddingGemma-300m (Gemma Terms of Use — fails permissive requirement), Qwen3-Embedding-0.6B / nomic-embed / arctic-m f32 (budget), potion/model2vec (quality tier too low for a quality-first mandate; noted as a possible future `--tiny` build).

**Parity gate:** the model choice is finalized only after Spike S1 shows cosine similarity ≥ 0.999 between sqlite-mem embeddings and the reference sentence-transformers implementation across a test corpus.

## 8. Packaging strategy

- Weights embedded via `include_bytes!` of the f16 safetensors (+ tokenizer.json + config): **one file, no extraction, no first run, no network**. Rodata is paged from the mapped executable, so RSS stays modest.
- Known costs accepted: slower release links, macOS notarization time. Mitigation: dev builds default to a `model-sidecar` feature (load from disk path) so incremental compiles stay fast; only release CI uses `--features embed-model`.
- Per-target artifacts (no macOS universal binary — it doubles a ~120MB payload).

## 9. Cross-platform build/release

Targets, in priority order:

1. `aarch64-apple-darwin`, `x86_64-apple-darwin` (signed + notarized)
2. `x86_64-unknown-linux-musl` (fully static), `aarch64-unknown-linux-musl`
3. `x86_64-pc-windows-msvc`

GitHub Actions matrix modeled on Satchel's `release.yml` (model fetched in CI, checksummed, embedded). Every release publishes SHA-256 checksums. Linux CI installs `musl-tools`; release builders need ≥ 10 GB RAM (linking a 95 MB `include_bytes!` peaks ~9.2 GB). *(Amended per S1:)* Determinism gate restated — raw embedding vectors differ in the last float ulp across libms (gnu vs musl on one machine already differ), so byte-identity is required of the **rounded JSON output** (scores at 5 decimals) on every platform, with float-tolerance (cosine ≥ 0.9999999) on raw vectors.

## 10. Persistent storage model and schema

One SQLite file. Proposed DDL (v1; managed by refinery-style forward-only migrations with `PRAGMA user_version` sync and a pre-migration `.bak` copy):

```sql
-- schema_version lives in PRAGMA user_version
CREATE TABLE db_info (            -- singleton system metadata
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);  -- rows: embedder_id, embedder_dims, embedder_content_hash,
    --       created_by_version, db_created_at

CREATE TABLE memories (
  id            TEXT PRIMARY KEY,          -- ULID (sortable, collision-free)
  content       TEXT NOT NULL,
  content_hash  TEXT NOT NULL,             -- sha256 hex; dedup + integrity
  source        TEXT,                      -- caller-supplied provenance string
  created_at    TEXT NOT NULL,             -- UTC RFC3339
  status        TEXT NOT NULL DEFAULT 'active',
                -- 'active' | 'superseded' | 'forgotten'
  superseded_by TEXT REFERENCES memories(id),
  forgotten_at  TEXT
);
CREATE INDEX idx_memories_status ON memories(status);
CREATE INDEX idx_memories_hash   ON memories(content_hash);

CREATE TABLE memory_meta (                  -- caller metadata, flat string map
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  key       TEXT NOT NULL,
  value     TEXT NOT NULL,
  PRIMARY KEY (memory_id, key)
);
CREATE INDEX idx_meta_kv ON memory_meta(key, value);

CREATE TABLE chunks (
  id        TEXT PRIMARY KEY,               -- '{memory_id}:{index}'
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  idx       INTEGER NOT NULL,
  text      TEXT NOT NULL,
  embedding BLOB NOT NULL                   -- little-endian f32 × dims
);
CREATE INDEX idx_chunks_memory ON chunks(memory_id);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
  text, content='chunks', content_rowid='rowid',
  tokenize='porter unicode61 remove_diacritics 2'
);
-- AI/AD/AU triggers keep chunks_fts in sync (Satchel pattern),
-- plus a backfill repair pass on open.
```

Rationale for load-bearing choices:

- **Vectors as plain BLOBs, brute-force cosine in Rust — no sqlite-vec in v1.** Two of the three inspected candidates independently found sqlite-vec unnecessary or harmful at memory scale (sqlite-graphrag dropped it in a dedicated migration; Satchel scans 50K chunks in <10ms). Brute force removes a pre-1.0 C extension, its on-disk-format churn risk, and all static-linking friction on musl/Windows. The `VectorIndex` trait seam (§13) makes adding sqlite-vec or HNSW a contained change if benchmarks ever demand it (not expected below ~100K chunks).
- **Metadata as an EAV table of flat string pairs, not a JSON column.** Filters become indexed SQL (`key = ? AND value = ?`), not `LIKE` over JSON text (rag-ferrite's weakness). Arbitrary keys, no ontology, no schema migration when callers invent conventions. Nested/typed metadata is deliberately deferred; strings cover the illustrative ontology in the product definition.
- **Soft delete** (`forgotten`) before hard delete (`purge` via `forget --purge`): autonomous callers make mistakes; recoverability is a safety feature.
- **Three metadata classes, physically separated:** caller-supplied (`memory_meta`), system/provenance (`memories` columns + `db_info`), retrieval/ranking (computed at query time, returned in JSON, never stored).

## 11. SAVE protocol

```
sqlite-mem save [--db PATH] [--meta KEY=VALUE]... [--source STR]
                [--supersedes ID]... [--if-new] (--content TEXT | --stdin)
```

Behavior:

1. Validate: content non-empty after trim; ≤ 1 MiB (hard cap, error `input_too_large`); ≤ 64 metadata pairs, key ≤ 128 bytes matching `[A-Za-z0-9_.-]+`, value ≤ 4 KiB. Keys are data — never interpolated into SQL (parameterized throughout).
2. Dedup: if an `active` memory with identical `content_hash` exists, return it with `"deduplicated": true` instead of inserting (idempotent saves — safe for AI retry loops). `--if-new` makes insertion the only success path (a duplicate then fails: exit 3, code `not_new`). **Dedup does not skip supersession:** any `--supersedes` targets are still marked superseded, with `superseded_by` pointing at the existing memory's id — a retried save must retire old truth exactly as the first attempt would have (self-supersession is ignored: a memory never supersedes itself).
3. Chunk: ≤ 1024 tokens per chunk, 64-token overlap, paragraph-boundary-preferring (Satchel's property-tested algorithm). Most distilled memories are one chunk.
4. Embed each chunk (query/document prefixes per model card, applied internally).
5. Single `IMMEDIATE` transaction: insert memory, metadata, chunks, FTS rows; mark each `--supersedes` target `status='superseded', superseded_by=<new id>`.
6. Respond on stdout:

```json
{"ok": true, "op": "save", "id": "01J8ZQ8XK3T4YV9RurW2K1QF5A",
 "deduplicated": false, "chunks": 1,
 "content_hash": "sha256:6c02...",
 "created_at": "2026-08-31T17:04:12Z",
 "superseded": ["01J8ZP..."],
 "embedder": {"id": "granite-embedding-small-english-r2", "dims": 384}}
```

## 12. ASK protocol

```
sqlite-mem ask [--db PATH] [--k N (default 5, max 50)]
               [--where KEY=VALUE]... [--where KEY!=VALUE]...
               [--include-superseded] [--include-forgotten]
               [--mode hybrid|lexical|semantic (default hybrid)]
               [--min-score F] (--query TEXT | --stdin)
```

- Filter grammar is deliberately tiny: repeated `--where` terms are ANDed; `KEY=VALUE` equality, `KEY!=VALUE` exclusion, and `KEY=*` existence. No boolean OR grammar, no ranges, no query language in v1 — callers wanting OR run two asks. (Deferred: `--prefer KEY=VALUE` soft-boost; see §22.)
- Status filtering: `active` only by default; superseded/forgotten included only on explicit flags, and always labeled.
- Returns **evidence only** — full memory content (distilled memories are small), never a synthesized answer.

```json
{"ok": true, "op": "ask", "mode": "hybrid",
 "query": "Why didn't we use that agent framework?",
 "results": [
   {"id": "01J8ZQ8XK3T4YV9RW2K1QF5A",
    "content": "We rejected Mastra because suspend/resume durability violated the Factory invariants.",
    "score": 0.03202, "ranks": {"lexical": 4, "semantic": 1},
    "metadata": {"project": "factory", "kind": "decision", "status": "current"},
    "system": {"created_at": "2026-08-14T09:12:44Z", "source": "decisions.md#D012",
               "status": "active", "content_hash": "sha256:9ab1..."}}
 ],
 "stats": {"candidates": 812, "returned": 1, "elapsed_ms": 41}}
```

Determinism contract: fixed serde field order; scores rounded to 5 decimals; total order = (score DESC, id ASC) so ties never reorder across runs; `elapsed_ms` is the only intentionally nondeterministic field.

## 13. Retrieval pipeline

```
query → [resolve filters → allowed-ID set]      (one indexed SQL pass)
      → lexical leg:  (≥ 4096 allowed chunks only — below that the default
                      ranks purely semantically, per the S5c ruling under
                      D016) FTS5 MATCH, bm25(), query tokens DF-filtered
                      (tokens matching > 50% of allowed chunks dropped,
                      floor df ≤ 2 kept), restricted to allowed set,
                      top min(200, max(4k, chunks/10))
      → semantic leg: embed query → cosine over ALL allowed chunks (untruncated)
      → RRF fusion:   score(c) = Σ_legs 1/(60 + rank_leg(c))
      → collapse chunks → best-scoring chunk represents its memory
      → sort (score DESC, id ASC) → top k → hydrate content + metadata
```

- **Allowed-ID set resolved once and shared by both legs** (rag-ferrite pattern) — filters constrain retrieval instead of starving post-fusion results.
- **Lexical:** FTS5 `bm25()`; query sanitized by token-quoting OR construction (Satchel's `build_fts5_query` — user text can never inject FTS5 syntax); document-frequency filtering and the corpus-scaled cap per the S5b/S5c changelog entries; the leg activates only at ≥ `LEXICAL_ACTIVATION_CHUNKS` (4096, an empirically calibrated revisable policy — see the changelog's empirical-policy note).
- **Semantic:** untruncated brute-force cosine over the filtered corpus (Satchel pattern; protects deep-filter recall; <10ms at 50K chunks).
- **Fusion:** RRF k=60 — three independent codebases and current literature converge on it; per-leg ranks are returned in JSON for debuggability and benchmark ablation.
- `--mode lexical|semantic` runs a single leg (needed for benchmarks; also the degraded path if the embedder is ever unavailable).
- `VectorIndex` trait seam isolates the cosine scan so sqlite-vec/HNSW can slot in later without schema change (embeddings stay in the BLOB column either way).

## 14. Authority, freshness, supersession

- sqlite-mem is mechanically honest but epistemically humble: it reports `created_at`, `status`, `superseded_by`, provenance, and caller metadata; the calling AI decides authority. It never ranks by recency by default (relevance ≠ freshness; conflating them is how stale-but-wordy memories win).
- Supersession is explicit and caller-driven (`--supersedes`), forming chains: old memory keeps its content (immutable history), gains `superseded` status, and is excluded from default retrieval (current-value semantics). No automatic staleness inference in v1.
- There is no `update` verb: memories are immutable; the update semantic is save-with-supersede. This keeps provenance trustworthy.

## 15. Deletion / forget

- `sqlite-mem forget [--db PATH] ID... [--purge]` — default marks `status='forgotten'` (excluded everywhere by default, recoverable via `forget --restore ID`); `--purge` hard-deletes rows, chunks, FTS, and metadata in one transaction. Purge is the only destructive operation in the product and says so in its JSON response.
- Junk-drawer control = dedup at save + supersession + forget + `info` reporting counts by status/age so a caller (or Folder Chief hygiene routine) can review and prune.

## 16. Concurrency and locking

WAL + `busy_timeout=5000` + short `IMMEDIATE` write transactions handles the realistic case (a few concurrent harness processes) with SQLite's own guarantees; readers never block. Multi-process stress test (8 writers × 100 saves + concurrent asks) is a v1 acceptance gate. No flock layer unless that test demands it (sqlite-graphrag's flock slots are the known fallback pattern).

## 17. Error model and exit codes

All errors are JSON on stdout (`{"ok": false, "error": {"code": "...", "message": "...", "hint": "..."}}`); logs/tracing go to stderr only (sqlite-graphrag's test-gated sink discipline). Exit codes:

| Code | Meaning |
|---|---|
| 0 | success |
| 2 | usage / invalid arguments |
| 3 | validation failure (oversized input, bad metadata key) |
| 4 | not found (unknown memory ID) |
| 5 | database error (locked past timeout, corrupt, permissions) |
| 6 | version/compatibility (schema newer than binary, embedder mismatch without `reindex`) |
| 7 | integrity check failure |

An integration test asserts the full code table and that stdout is always exactly one parseable JSON document.

## 18. Observability

`--verbose` → structured tracing on stderr (never stdout). `info` reports: schema version, embedder id/dims, memory counts by status, chunk count, db size, oldest/newest timestamps. `info --verify` runs `PRAGMA integrity_check`, FTS backfill check, embedding-dimension audit, and hash spot-checks. On any check failure it exits 7 with `ok:false` and `error.code="integrity_failed"` (message naming the failed checks), plus a top-level `checks` object carrying per-check `{pass, detail}` results — the invariant is uniform: **every non-zero exit pairs with `ok:false`**; a passing verify exits 0 with `ok:true` and the same `checks` detail.

## 19. Migrations, model upgrades, recovery

- **Schema:** forward-only SQL migrations, `PRAGMA user_version`, automatic timestamped `.bak` pre-migration copy (sqlite-graphrag pattern). A binary refuses (exit 6) to write a DB whose schema is newer than itself.
- **Embedder identity** is recorded in `db_info` at DB creation. A binary whose bundled embedder differs from the DB's: `save`/`ask --mode semantic|hybrid` fail with exit 6 and a hint; `ask --mode lexical` still works; `sqlite-mem reindex` re-embeds every chunk with the current embedder and updates `db_info` (in a transaction, with `.bak` first). Content is authoritative; embeddings are always rebuildable — this is the recovery story for vector corruption too.
- **Tampered/corrupt DBs:** detected via `info --verify`; recovery = restore `.bak`, or export content (`sqlite3` works on the file) and re-save.

## 20. Safety and security boundaries

- **Network: structurally impossible, not just prohibited.** No HTTP/DNS/socket crate appears anywhere in the dependency tree; CI gates on `cargo tree` matching a denylist (reqwest, hyper, tokio-net, ureq, ...). This is the product's headline guarantee.
- **Content is data, never instructions.** sqlite-mem never interprets, executes, templates, or evaluates stored content; retrieved text is returned verbatim inside JSON string encoding. Prompt-injection risk in stored text is real but belongs to the caller; the README and `ask` docs must state plainly: *treat retrieved content as untrusted data, like file contents*.
- **SQL injection:** parameterized statements only; FTS5 queries built by the token-quoting sanitizer; metadata keys validated against `[A-Za-z0-9_.-]+`.
- **DoS/resource caps:** 1 MiB content, 4 KiB metadata values, k ≤ 50, query ≤ 8 KiB. Oversized input → exit 3, no partial writes.
- **Paths:** DB path from `--db`, else `SQLITE_MEM_DB` env, else `./.sqlite-mem/memory.db`. Parent directory must already exist except for the default path (created `0700`); DB files created `0600`. The binary opens only: the DB, its WAL/SHM, and its `.bak` — nothing else, ever. Symlinked DB paths are followed (user-owned file, user's choice) but `info` reports the resolved path.
- **Safe defaults:** soft delete before purge; dedup before insert; refusal (not silent fallback) on version mismatch.

## 21. Test and benchmark architecture

**Tests** (patterns lifted from the best of each candidate):

- Unit: chunker property tests (no word loss, offsets valid — Satchel's proptest suite), FTS sanitizer fuzz, RRF math, metadata validation, blob↔vec roundtrip.
- Integration: full CLI contract (every verb, every exit code, stdout-is-one-JSON-document gate, no-stray-println gate — sqlite-graphrag pattern), migration upgrade/downgrade-refusal, embedder-mismatch behavior, multi-process concurrency stress.
- Determinism: identical DB + identical query ⇒ byte-identical output, cross-run and cross-platform (float tolerance on Windows only if measured necessary).
- Embedding parity: bundled-runtime vectors vs Python sentence-transformers reference, cosine ≥ 0.999 (fixture vectors checked in).

**Benchmarks** (harness modeled on rag-ferrite's `benchmark.rs`; golden dataset in-repo):

1. **Kernel proof:** ≥ 50 memory corpus with realistic Folder Chief distillations; ≥ 30 cross-wording queries (the Mastra example is query #1) where gold answers share little vocabulary with queries. Gate (*recalibrated per D016.3 on S5b/S5c evidence — original 0.85/0.70 was set pre-evidence; achieved ceiling is the embedder's, measured 0.8114/0.6697 on a 71%-zero-overlap adversarial dataset*): default-mode recall@5 ≥ 0.80 and MRR ≥ 0.65 on the golden benchmark, and the default mode ≥ each pure mode at every measured scale (62 / 1K / 10K chunks).
2. **Ablations:** lexical vs semantic vs hybrid; filtered vs unfiltered; primary model vs fallback model (drives the final model decision).
3. **Ops metrics:** cold-start-to-first-byte (< 1.5s gate on the release binary), warm ask latency (*restated per D016:* end-to-end < 1s at 10K chunks AND retrieval-only lexical path < 50ms — per-invocation model load is ~500ms flat and inherent to the transient-CLI model), save latency, peak RSS, binary size (≤ 150MB gate).
4. **Folder Chief value metric:** token-cost comparison — tokens returned by `ask` vs tokens an AI reads exploring the equivalent file tree for the same 30 questions (measured with a real harness, reported not gated).

## 22. Dependencies and licensing

Runtime deps (all MIT and/or Apache-2.0): `rusqlite` (bundled), `candle-core/nn/transformers`, `tokenizers`, `serde`/`serde_json`, `clap`, `ulid`, `sha2`, `tracing`. Model: Apache-2.0 (granite) or MIT (bge fallback). sqlite-mem itself: **MIT OR Apache-2.0 dual**. Reused Satchel/sqlite-graphrag code retains upstream MIT/Apache attribution in a `THIRD-PARTY.md`. rag-ferrite has no LICENSE file — patterns only unless the author confirms MIT. RavenRustRAG is AGPL — no code, ever.

## 23. Explicitly deferred (with the seam that admits each)

| Deferred | Seam |
|---|---|
| MCP server | none needed — any MCP wrapper can shell out to the CLI |
| sqlite-vec / ANN | `VectorIndex` trait; embeddings already in BLOB column |
| `--prefer` soft metadata boosts, OR-filters, ranges | filter resolver is one module |
| Neighbor-chunk context expansion | chunk table has `idx`; pure query change |
| Multilingual model / `--tiny` model2vec build | embedder id in `db_info` + `reindex` |
| Typed/nested metadata | EAV values are strings; add `type` column later |
| Time-decay / authority-aware ranking | ranks already returned; fusion is one function |
| Batch save | protocol change only |
| Encryption at rest | out of scope; the file is user-owned |

Everything above failed the "does SAVE or ASK actually require this?" test for v1.

## 24. Architectural invariants

1. One user-owned SQLite file is the only persistent state; standard format, always openable by stock sqlite3.
2. The process is transient: start → operate → exit. Never a daemon, server, or agent.
3. Zero network capability in the dependency tree — enforced by CI, forever.
4. No user-configured model, provider, key, or runtime — the binary embeds everything.
5. stdout carries exactly one deterministic JSON document per invocation; diagnostics go to stderr.
6. Stored content is data: never executed, interpreted, or silently mutated; memories are immutable (supersede, don't edit).
7. Content is authoritative in the DB; embeddings and FTS are always rebuildable derivatives.
8. The caller owns cognition and authority judgments; sqlite-mem owns mechanics and reports evidence.
9. Two cognitive primitives; mechanical verbs only for lifecycle/integrity; retrieval surface does not grow without a demonstrated need and an accepted decision.

## 25. Definition of Done for v1

- All five platform binaries build in CI, ≤ 150MB, checksummed, macOS signed/notarized.
- Full CLI contract implemented (`save`, `ask`, `forget`, `reindex`, `info`) with the documented JSON schemas and exit codes; contract tests green.
- Embedding parity ≥ 0.999 vs reference; determinism tests green cross-platform.
- Kernel-proof benchmark gates met (per D016.3 recalibration: default-mode recall@5 ≥ 0.80, MRR ≥ 0.65, default ≥ each pure mode at every measured scale); ablation report published in-repo.
- Cold start < 1.5s; warm ask per D016.2: end-to-end < 1s at 10K chunks and retrieval-only (lexical path) < 50ms.
- Concurrency stress test green; migration + reindex + recovery paths tested.
- Network-denylist CI gate active; security checklist (§20) audited by an independent review pass.
- README with the save/ask contract, Folder Chief conventions guide, and THIRD-PARTY attributions.

## 26. Material uncertainties and the experiments that close them

1. ~~**Candle × ModernBERT parity**~~ **CLOSED by S1 (2026-08-31):** exact parity (min cosine 1.000000000 at 9 dp, all 100 texts, both models); see `spike/embed-parity/REPORT.md`. Model adoption awaits Lee's G1 decision.
2. **Retrieval quality of a 47M model on distilled-memory text** → benchmark ablation (Sprint 5); fallback = arctic-embed-m-v1.5 int8 via `ort` (budget-fit, higher quality, higher build cost) — would be a new decision for Lee.
3. **rag-ferrite license confirmation** → one GitHub issue; until answered, patterns only.
4. ~~**Windows float determinism**~~ Largely closed by S1's finding that ulp-level divergence exists even between Linux libms: the contract is rounded-output byte-identity everywhere (§9). Sprint 6 verifies Windows meets it.

## Changelog

- **2026-08-31 (post-S1):** §7 amended (Candle C-dep reality, required `modernbert_mem` module), §9 amended (musl-tools, builder RAM, determinism restated as rounded-output byte-identity), §26 items 1 and 4 closed. Evidence: `spike/embed-parity/REPORT.md`.
- **2026-08-31 (S2 review):** §11.2 clarified from implementation review: dedup still applies `--supersedes` (retry-safety must not drop retire-intent); `--if-new` duplicate = exit 3 code `not_new`; missing parent dir for an explicit `--db` = exit 5 code `db_path_unavailable`.
- **2026-09-01 (empirical-policy note, per Lee):** `LEXICAL_ACTIVATION_CHUNKS = 4096` is an **empirically calibrated v1 policy, not a universal constant**. It encodes a measured scale-dependent retrieval regime (semantic-only wins at 62 and 1K chunks; hybrid wins at 10K; the true crossover lies somewhere between 1K and 10K and was bracketed, not located) from exactly three data points on one corpus family with one embedder. It must be re-derived — not defended — whenever benchmark evidence, the embedder, or the fusion mechanics change; the constant is a single named value at one site in `ask.rs` precisely so revision is a measurement exercise, not a redesign. Likewise on the same instruction: the §21.1 gate recalibration is recorded as **evidence-driven recalibration after bounded tuning failed** (adversarial 71%-zero-overlap benchmark; measured 47M-embedder ceiling 0.8114/0.6697; crossover observed across corpus sizes; default never worse than either pure mode at any measured scale) — not a casual goalpost move.
- **2026-09-01 (S5b/S5c close, D016.3 applied):** §13 lexical leg finalized: DF query-token filtering (>50% of allowed chunks dropped, absolute floor df≤2), corpus-scaled candidate cap min(200, max(4k, chunks/10)), and **lexical-leg activation only at ≥ 4096 allowed chunks** — below that the default mode ranks purely semantically (the corpus-scaled cap taken to its measured limit; tuning within D016.1 bounds could not make small-corpus fusion beat semantic, and the invariant "default ≥ each pure mode" outranks keeping a noisy leg). §21.1/§25 gates recalibrated per D016.3 to achieved evidence: recall@5 ≥ 0.80 / MRR ≥ 0.65 (measured 0.8114/0.6697) + default ≥ each pure mode at 62/1K/10K. Retrieval-only latency verified 23–34ms at 10K (< 50ms gate). Evidence: bench/REPORT.md §S5b/§S5c.
- **2026-08-31 (G2/D016):** §21.3 warm-latency gate restated (split end-to-end vs retrieval-only; the 250ms gate conflated flat model load with ~3ms retrieval). §13 lexical leg to gain document-frequency query-token filtering + a corpus-scaled candidate cap (S5b, D016) — the S5 benchmark showed stopword OR-joins let the lexical leg match most of a small corpus, feeding rank noise into RRF; the effect inverts at 10K chunks, so the fix must preserve the at-scale crossover.
- **2026-08-31 (S3 review):** §12 example score corrected to match the RRF formula (0.03252 → 0.03202; worker-caught arithmetic error in the illustration, not a formula change). §12 clarifications codified from accepted judgment calls: empty query = exit 3 `empty_query`; `--k` range 1–50 (violation = exit 2); malformed `--where` = exit 2; `content` hydrates the full memory; `candidates` = fused pre-collapse chunk-union size.
- **2026-08-31 (S2 close):** Candle pinned to **0.9.1 with tokenizers/fancy-regex** (the §7 open question): the S1 parity harness re-run against that exact configuration passed on all 100 texts (min cosine ≥ 0.999999), and the dependency tree is now fully pure-Rust — no oniguruma, so musl builds need no C toolchain. candle 0.11 + musl-tools remains the documented fallback if 0.9.x maintenance becomes a risk.
