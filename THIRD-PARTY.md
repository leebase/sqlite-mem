# Third-party attributions

sqlite-mem is MIT OR Apache-2.0 (decisions.md D011). This file records code
and model provenance for material ported or adapted from other permissively
licensed projects, per architecture.md §22 and project-plan.md's
buy/build/fork/steal checklist. Each port site also carries an inline
comment pointing back here.

## Satchel

**Project:** virgilvox/satchel
**License:** MIT
**Copyright:** (c) 2026 Moheeb Zara

Ported/adapted into this crate:

- `src/embed/mod.rs` — the Candle BERT/embedder loader shape (disk-path
  resolution, `VarBuilder::from_mmaped_safetensors`, CLS pooling, L2
  normalization, and the `Fixed` deterministic test embedder pattern behind
  a `test-support` feature flag) is adapted into
  `sqlite-mem/src/embed/mod.rs`. The bundled model differs (granite ModernBERT
  vs. Satchel's bge-small BERT), per decisions.md D014, so the loader was
  rewritten around the S1 spike's Candle configuration rather than copied
  verbatim; the pooling/normalization/test-embedder shape is Satchel's.
- `src/ingest/mod.rs::chunk_text` and its property tests — ported nearly
  verbatim into `sqlite-mem/src/chunk.rs` (parameters changed to the
  product's 1024-token / 64-token-overlap bound per architecture.md §11 and
  decisions.md D014; the paragraph-preferring algorithm, the `len/4` token
  approximation, and the property tests are Satchel's).

## sqlite-graphrag

**Project:** danilo-aguiar-br/sqlite-graphrag
**License:** MIT OR Apache-2.0
**Copyright:** (c) 2026 Danilo Aguiar

Patterns adapted into this crate:

- `src/output/` — the single-stdout-sink discipline (`write_line`,
  `BrokenPipe`-is-success, one JSON document per invocation, all logging via
  `tracing` to stderr, error envelopes with a hand-rolled serializer
  fallback) is adapted (substantially simplified to this product's exact
  §17 envelope shape) into `sqlite-mem/src/output.rs`.
- Migration runner shape — forward-only migrations keyed by
  `PRAGMA user_version`, with a timestamped pre-migration `.bak` copy — is
  the same pattern sqlite-graphrag uses (`src/commands/migrate.rs`),
  reimplemented in `sqlite-mem/src/db/mod.rs` against this product's own
  (much smaller) migration set.

## candle / candle-transformers

**Project:** huggingface/candle
**License:** Apache-2.0 OR MIT

`src/embed/modernbert_mem.rs` (the memory-efficient ModernBERT forward pass
required by architecture.md §7 and decisions.md D014) is derived from
`candle-transformers` 0.11.0's `src/models/modernbert.rs`, carried in
unchanged from the S1 spike (`spike/embed-parity/rust/src/modernbert_mem.rs`).
See that file's own header comment for the exact derivation (per-head
attention, fused softmax, skipped no-op padding mask) and
`spike/embed-parity/REPORT.md` finding F1 for why the stock module cannot
ship (16.3 GB peak RSS vs. 1.42 GB for this module).

## Models

- **granite-embedding-small-english-r2** (ibm-granite) — Apache-2.0. The
  bundled/sidecar embedding model (decisions.md D014). Pinned Hugging Face
  revision `2ab6fa8ea2d674564defd37171ae19079b864b33`; see
  `.github/workflows/release.yml` for the exact fetch + sha256-verify +
  f16-conversion steps release CI runs.
- **bge-small-en-v1.5** (BAAI) — MIT. Validated fallback (decisions.md
  D014), not wired into the product in Sprint S2.

## Direct runtime dependencies

Every crate the shipped binary links against (dev-only dependencies such as
`assert_cmd`/`predicates`/`proptest`/`tempfile` never appear in a built
binary and are omitted). All are MIT and/or Apache-2.0; none carries
attribution obligations beyond what Cargo already vendors as each crate's
own `LICENSE-*` file(s). Versions per `Cargo.lock` as of Sprint S6; license
identifiers per `cargo metadata`.

| Crate | Version | License |
|---|---|---|
| rusqlite (bundled SQLite) | 0.40.2 | MIT |
| candle-core / candle-nn / candle-transformers | 0.9.1 | MIT OR Apache-2.0 |
| tokenizers | 0.22.2 | Apache-2.0 |
| clap | 4.6.6 | MIT OR Apache-2.0 |
| serde / serde_json | 1.x | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| ulid | 1.2.1 | MIT |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | MIT |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| thiserror | 1.0.69 / 2.0.20 | MIT OR Apache-2.0 |

**sqlite-vec — not used.** Considered and rejected per architecture.md §10
("Vectors as plain BLOBs, brute-force cosine in Rust — no sqlite-vec in
v1"); no sqlite-vec code or binary is linked into this product. Listed here
only to close out the buy/build/fork/steal checklist item, not because any
notice is owed.
