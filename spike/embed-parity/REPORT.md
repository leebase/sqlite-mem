# Sprint S1 Report — Embedding Parity & Packaging Spike

**Date:** 2026-08-31 · **Status:** Complete — all acceptance criteria met · **Gate G1 decision:** requested from Lee (recommendation below)

## Verdict

**Candle can run granite-embedding-small-english-r2 (ModernBERT) correctly, fully offline, inside a 104 MiB binary, with 0.57 s cold start and exact parity with the Python reference.** One deviation was required (memory-efficient attention, below). The fallback (bge-small-en-v1.5) also passes parity exactly.

## Acceptance criteria

| Criterion (project-plan.md S1) | Gate | Measured | Result |
|---|---|---|---|
| Parity vs sentence-transformers (granite) | cosine ≥ 0.999 all 100 texts | min 1.000000000 (9 dp) | **PASS** |
| Parity vs reference (bge fallback) | cosine ≥ 0.999 all 100 texts | min 1.000000000 (9 dp) | **PASS** |
| Binary with embedded weights | ≤ 150 MB | 109,214,072 B (104 MiB) gnu; 108 MiB musl | **PASS** |
| Cold start (spawn → embedding) | < 1.5 s | 0.57 s median (embedded weights) | **PASS** |
| musl cross-compile smoke | builds | static-pie ELF builds & runs, parity 0.99999994 vs gnu | **PASS** (workaround needed, see F2) |

## Method

- Fixture corpus: `fixtures/corpus.jsonl` — 100 deterministic texts from repo prose (seed 20260831), 7–6,657 words, including 5 texts >4096 tokens; t001 is the kernel-proof Mastra memory.
- Reference: sentence-transformers 6.0.1 / torch 2.13.0+cpu, forced true-f32 compute (granite's config declares bf16 and ST silently honors it — caught and corrected), normalize_embeddings=True, no prefixes, default truncation. `python/reference.py`, notes in `python/MODEL-NOTES.md`.
- Candle side: Candle 0.11.0, f32 compute (bf16 checkpoint upcast at load), CLS pooling per each model's `1_Pooling/config.json`, HF-matching truncation (max−2 content + [CLS]/[SEP]), L2 normalization. `rust/src/main.rs`.
- Comparison: `compare.py` (stdlib cosine, ≥0.999 gate). Both runs bit-reproducible.

## Key findings

**F1 — Stock Candle ModernBERT cannot ship.** It materializes a dense (1,12,8192,8192) f32 attention tensor + unfused softmax: 16.3 GB peak RSS, OOM-killed on the first 8192-token text. A 130-line derived module (`rust/src/modernbert_mem.rs`: per-head attention + fused `softmax_last_dim`) drops peak RSS to 1.42 GB, is ~24% faster, and matches stock to 8.2e-8 max component diff on all texts stock survives. This module must be carried into S2 (candidate for upstreaming to Candle).

**F2 — "Pure Rust inference" was wrong as written.** `candle-core` ≥ 0.10 unconditionally pulls `tokenizers` with the `onig` feature → `onig_sys` (C oniguruma). musl builds need a real musl C toolchain (`musl-tools` in CI) or the CC/CFLAGS workaround documented in the agent report; alternatively pin `candle-core` 0.9.1 (no tokenizers dep; its ModernBERT is byte-identical for our path). Not a network/denylist risk — a build-complexity fact. → architecture.md §7 amended.

**F3 — Byte-identical cross-platform determinism is unattainable at the vector level.** gnu vs musl on the same machine differ in the last float ulp (libm). Irrelevant after `ask`'s 5-decimal score rounding, but the §9/§21 gates are restated as: byte-identical *JSON output after rounding*, float-tolerance on raw vectors. → architecture.md amended.

**F4 — Sizes and timings** (12-core WSL2):

| Measure | Value |
|---|---|
| granite hub checkpoint | 95.3 MB (**bf16 on disk**); f16 conversion lossless, same size |
| binary, no model | 10.3 MB |
| **binary + embedded granite f16** | **104 MiB** |
| model load: sidecar mmap / include_bytes | 216 ms / ~300 ms |
| embed ≤128 tok / ≤512 tok | 281 ms / 451 ms (granite) |
| embed 8192 tok | ~39 s — confirms `save` must chunk (≤1024 tok) before embedding |
| corpus RSS peak | 1.42 GB (granite, incl. >4k-token texts) |
| link with 95 MB include_bytes | 6.3–7.5 s, ~9.2 GB build RSS — CI builders need ≥10 GB RAM |
| f16-weights vs f32 run (t001) | cosine 1.0000000 (f32 compute) |

**F5 — Model facts confirmed:** granite Apache-2.0, CLS pooling, 8192 ctx, ModernBERT BPE tokenizer, no prefix scheme, and its `modules.json` has **no normalize module** (product must L2-normalize itself — we do). bge MIT, CLS, 512 ctx, optional query prefix noted for product use in `python/MODEL-NOTES.md`.

## G1 recommendation

**Adopt granite-embedding-small-english-r2 (f16, embedded) as the v1 model; retain bge-small-en-v1.5 as the tested fallback.** Both pass parity exactly; granite wins on quality tier, 8192-token headroom, and budget fit (104 MiB, 2.6× cold-start headroom). Conditions: carry `modernbert_mem.rs` into S2; decide candle pin (0.11 + musl-tools vs 0.9.1) in S2 setup.

## Reproduction

See "Re-run commands" in the Rust agent report (mirrored in `rust/README` comments); parity: `python3 compare.py out/candle_<m>.json out/reference_<m>.json`. Artifacts: `out/*.json` (4 vector sets), `models/` (weights, gitignored), `rust/` (spike crate).
