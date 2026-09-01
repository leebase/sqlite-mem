# sqlite-mem benchmark report — Sprint S5 kernel proof

**Date:** 2026-08-31 · **Binary:** release profile, `model-sidecar` feature, built from `b2683cc` (S3 close) in an isolated worktree · **Model:** granite-embedding-small-english-r2, f32 sidecar, 384 dims · **Host:** Linux x86_64 (WSL2), 12 logical cores.

> **Headline: three of the six retrieval gates FAIL at the mandated corpus size,
> plus the warm-latency gate.**
>
> 1. **Fusion is mis-tuned for small corpora.** At the 62-memory kernel-proof
>    scale, semantic-only beats the shipped default `hybrid` on every metric, and
>    hybrid is *worse* than its own semantic leg on 13 of 38 queries. The cause is
>    identified and reproducible: the lexical leg ranks 81% of a small corpus on
>    stopword mass, and rank-based RRF gives that noise the same weight as a true
>    top-ranked hit (§3.4).
> 2. **The effect inverts with scale.** Re-running the identical query set at
>    1,000 and 10,000 memories shows the deficit closing and then reversing — at
>    10K, **hybrid beats semantic on recall@5** and gate G-Q4 passes (§4). The §13
>    design is sound at scale; it is the small-corpus regime that breaks.
> 3. **Warm `ask` latency is ~532 ms at 10K against a 250 ms bar**, of which 1–3 ms
>    is retrieval and ~500 ms is loading the embedding model on every process
>    invocation. This gate is unachievable by any process-per-invocation CLI (§5.2).
>
> All of this is in the `architecture.md` §13/§21 **specification**, not in the S3
> implementation, which matches its spec faithfully and in which **no defect was
> found** across ~11,000 `save` and ~1,200 `ask` invocations.
>
> These are **G2 decision requests for Lee**, not worker-level fixes.

---

## 1. Gate verdicts

Thresholds quoted from `architecture.md` §21/§25 and `project-plan.md` S5. They
were fixed before any measurement and were not adjusted afterwards.

| # | Gate | Required | Measured | Verdict |
|---|---|---|---|---|
| G-Q1 | hybrid recall@5 ≥ 0.85 | ≥ 0.85 | **0.636** | ❌ **FAIL** |
| G-Q2 | hybrid MRR ≥ 0.70 | ≥ 0.70 | **0.497** | ❌ **FAIL** |
| G-Q3 | hybrid recall@5 ≥ lexical recall@5 | ≥ 0.412 | 0.636 | ✅ PASS |
| G-Q4 | hybrid recall@5 ≥ semantic recall@5 | ≥ 0.811 | **0.636** | ❌ **FAIL** |
| G-Q5 | hybrid MRR ≥ lexical MRR | ≥ 0.186 | 0.497 | ✅ PASS |
| G-Q6 | hybrid MRR ≥ semantic MRR | ≥ 0.670 | **0.497** | ❌ **FAIL** |
| G-O1 | cold start < 1.5 s | < 1500 ms | 452 ms (median of 5) | ✅ PASS |
| G-O2 | warm ask < 250 ms at 10K chunks | < 250 ms | **532 ms** (median, 10,000 chunks) | ❌ **FAIL** |
| G-O3 | binary ≤ 150 MB | ≤ 150 MB | 11.6 MB | ⚠️ not the gated build — see §5.4 |

All quality gates above are measured on the **62-memory kernel-proof corpus**,
which is the corpus `architecture.md` §21.1 specifies. §4 shows that G-Q4 flips to
PASS at 10,000 memories and G-Q6's deficit shrinks from 0.172 to 0.041 — so these
verdicts are specific to the mandated scale, not universal.

### What passes if the default mode changes

Run the identical corpus through `--mode semantic`:

| Gate | Required | `--mode semantic` | Verdict |
|---|---|---|---|
| recall@5 ≥ 0.85 | 0.85 | 0.811 | ❌ short by 0.039 |
| MRR ≥ 0.70 | 0.70 | 0.670 | ❌ short by 0.030 |

Semantic-only is **close to, but still under, both absolute bars**. Switching the
default would recover most of the gap without closing it, and would discard the
one query in 38 where the lexical leg is the only thing that finds the answer
(§3.3). Both facts belong in the G2 decision.

---

## 2. Dataset

Full rationale in [`corpus/DESIGN.md`](corpus/DESIGN.md).

| Property | Value |
|---|---|
| Memories | 62 (target ≥ 50), 5 fictional projects |
| Kinds | decision 21, constraint 13, preference 10, precedent 9, review 9 |
| Length | 1–2 sentences, median 30 words, max 47 |
| Queries | 38 (target ≥ 30) |
| Categories | cross-wording 15, adversarial 8, metadata-scoped 8, precedent 7 |
| Multi-gold queries | 4 — q08, q24, q25, q30 (target ≥ 3) |
| Holdout | 8 (21%), authored blind after harness freeze |
| **Zero content-word overlap with gold** | **27 of 38 (71%)** |
| Mean content-word overlap | 0.32 words per query |

The Mastra memory is present verbatim with the mandated metadata, and
`"Why didn't we use that agent framework?"` is query q01.

**The dataset is hard enough for the gates to mean something.** Lexical-only
reaches recall@5 = 0.412 against semantic's 0.811 — a 2× gap. The
`project-plan.md` failure mode "dataset too easy (lexical alone passes)" did not
occur.

---

## 3. Retrieval results

### 3.1 Overall (38 queries, unfiltered, k=5)

| Mode | n | recall@1 | recall@5 | MRR | nDCG@5 |
|---|---|---|---|---|---|
| **hybrid** (shipped default) | 38 | 0.3772 | 0.6360 | 0.4974 | 0.5170 |
| lexical | 38 | 0.0790 | 0.4123 | 0.1860 | 0.2390 |
| **semantic** | 38 | **0.5219** | **0.8114** | **0.6697** | **0.6864** |

Semantic wins every metric. Hybrid sits *between* its two legs rather than above
them — the opposite of what fusion exists to do.

### 3.2 Per category

| Category | Mode | n | recall@1 | recall@5 | MRR | nDCG@5 |
|---|---|---|---|---|---|---|
| adversarial | hybrid | 8 | 0.1875 | 0.4375 | 0.3167 | 0.3234 |
| adversarial | lexical | 8 | 0.0000 | 0.1875 | 0.0938 | 0.1119 |
| adversarial | semantic | 8 | 0.3750 | 0.6875 | 0.5833 | 0.5766 |
| cross-wording | hybrid | 15 | 0.3222 | 0.6444 | 0.4889 | 0.5027 |
| cross-wording | lexical | 15 | 0.0667 | 0.3444 | 0.1544 | 0.1976 |
| cross-wording | semantic | 15 | 0.5889 | 0.8889 | 0.7667 | 0.7680 |
| metadata-scoped | hybrid | 8 | 0.5000 | 0.6250 | 0.5250 | 0.5484 |
| metadata-scoped | lexical | 8 | 0.1250 | 0.6250 | 0.2604 | 0.3490 |
| metadata-scoped | semantic | 8 | 0.5000 | 0.6250 | 0.5625 | 0.5789 |
| precedent | hybrid | 7 | 0.5714 | 0.8571 | 0.6905 | 0.7330 |
| precedent | lexical | 7 | 0.1429 | 0.5714 | 0.2738 | 0.3472 |
| precedent | semantic | 7 | 0.5714 | **1.0000** | 0.6833 | 0.7597 |

- **The adversarial category did its job.** Lexical scores recall@1 = 0.0000 — on
  queries with zero content-word overlap it never once ranked the right answer
  first. That is precisely what the category was built to expose.
- **Precedent-seeking is the product's strongest use case.** Semantic finds every
  gold memory inside the top 5 (recall@5 = 1.000). `architecture.md` §3's "have we
  hit this before?" scenario is genuinely well served, and that deserves saying
  among the failures.
- **Cross-wording is where hybrid loses most**, giving up 0.244 recall@5 against
  semantic.

### 3.3 Ablation: where each leg rescues the other

| Situation | Count of 38 | Queries |
|---|---|---|
| Semantic rescues lexical (lexical MRR = 0, semantic > 0) | **16** | q02, q03, q06, q07, q10, q11, q15, q16, q17, q24, q26, q27, q29, q31, q32, q34 |
| Lexical rescues semantic (semantic MRR = 0, lexical > 0) | **1** | q35 |
| Both legs fail | 5 | q01, q20, q28, q37, q38 |
| Hybrid beats **both** legs | 4 | q05, q11, q13, q19 |
| Hybrid **worse** than semantic alone | **13** | q03, q04, q06, q07, q10, q15, q16, q17, q24, q26, q29, q31, q34 |

**Semantic rescuing lexical — the product claim working:**

> **q27** "What keeps the books straight when a charge has to be undone?"
> → m032 *"Refunds are recorded as new ledger entries rather than as reversals of
> the original…"* — shared content words: **zero**. `books`↔`ledger`,
> `charge undone`↔`refund` is pure paraphrase. Lexical MRR 0.000, semantic 0.333.

> **q34** (holdout) "What happens to an entry that turns out to be much too long?"
> → m043 *"Log lines must stay under 8 KB. The collector truncates anything longer
> mid-field…"* — lexical MRR 0.000, semantic 1.000. Authored blind, never
> test-driven.

**Lexical rescuing semantic — rarer, but real:**

> **q35** (holdout) "Where does the wording that drives the model live?"
> → m004 *"Lee prefers that agent prompts live in versioned files under prompts/…"*
> Semantic MRR 0.000; lexical catches it on the shared token `live` plus the
> `model`/`prompts` co-occurrence. This is the single query in 38 where deleting
> the BM25 leg outright would lose the answer — the argument against simply
> removing it.

**Hybrid destroying a correct semantic result — the failure:**

| Query | Gold | Gold semantic rank | Gold lexical rank | Hybrid rank |
|---|---|---|---|---|
| q03 "…keep small rounding errors out of what customers owe?" | m029 | **1** | 26 | 9 |
| q07 "…everything tried again at the same instant?" | m006 | **1** | 23 | 4 |
| q10 "Where do the words we use for colors come from?" | m056 | **1** | — | > 50 |
| q17 "What rules govern how we handle sensitive customer details?" | m033 | **1** | 21 | 6 |
| q24 "…the same instruction arrives twice because the wire hiccuped?" | m030 | 6 | 16 | 5 |
| q25 "Can a person who cannot use a mouse get all the way through?" | m054 | **1** | 30 | 5 |
| q29 "…decided while running that makes the price of a job unpredictable?" | m014 | **1** | 13 | 4 |

In five of these the semantic leg had the answer **at rank 1** and fusion pushed
it out of the top slot.

### 3.4 Root cause: the lexical leg has no noise floor

`build_fts5_query` (`src/ask.rs:171`) OR-joins every alphanumeric token in the
query with no stopword removal — exactly what §13 specifies ("token-quoting OR
construction"). The implementation is correct against its spec: bm25 is sorted
ASC (the documented sign trap is handled), RRF is `Σ 1/(60+rank)` with k=60, ties
break on chunk id. **The problem is what that specification does to
natural-language questions.**

Because every question contains `we`, `the`, `a`, `do`, `what`, the OR query
matches most of the corpus:

> **The lexical leg assigns a rank to a mean of 50.5 of the 62 memories (81%) per
> query.** Range across the 38 queries: 10 to 62.

RRF is rank-based and therefore scale-free, so a document ranked #1 by BM25 on
stopword mass contributes `1/61 = 0.01639` — the *identical* contribution a true
positive earns for being ranked #1 by cosine. Across 62 documents the whole
lexical spread (`1/61` down to `1/122`) is 0.0082, the same order of magnitude as
the whole semantic spread. Fusion therefore averages a strong signal with a
near-random one, and near-random wins about a third of the time.

Worked example — **q17** "What rules govern how we handle sensitive customer details?"

| Memory | lexical rank | semantic rank | RRF score |
|---|---|---|---|
| m012 (no agent may write outside its worktree) | **1** | 7 | 1/61 + 1/67 = 0.03132 |
| m033 (card data never enters our systems) ← **gold** | 21 | **1** | 1/81 + 1/61 = 0.02874 |

m012 is a *Factory isolation constraint* with nothing to do with customer data.
It reaches lexical rank 1 purely on `rules`/`govern`/`how`/`we` token mass, and
that is enough to beat a gold document the semantic leg ranked first.

---

## 4. Retrieval at scale — the RRF noise-floor control

The §13 lexical leg is capped at the top 200 chunks. **At 62 memories that cap
never binds**, so every noisy match survives into fusion. At larger corpora the
cap should exclude most noise and the pathology should weaken. That is a testable
prediction, so it was tested: the same 38 queries and gold labels re-run after
inflating the database with deterministic synthetic filler (seed 20260831; filler
is plausible engineering prose, not lorem ipsum, so it competes realistically).

| Corpus | Mode | recall@5 | MRR | hybrid − semantic (recall@5) |
|---|---|---|---|---|
| 62 | hybrid | 0.6360 | 0.4974 | **−0.175** |
| 62 | lexical | 0.4123 | 0.1860 | |
| 62 | semantic | 0.8114 | 0.6697 | |
| 1,000 | hybrid | 0.6228 | 0.5040 | **−0.031** |
| 1,000 | lexical | 0.3114 | 0.2237 | |
| 1,000 | semantic | 0.6535 | 0.6009 | |
| 10,000 | **hybrid** | **0.6140** | 0.5250 | **+0.026** |
| 10,000 | lexical | 0.3377 | 0.2057 | |
| 10,000 | semantic | 0.5877 | 0.5658 | |

**The prediction holds, and then some — the ordering reverses.**

| Corpus | lexical leg ranks | hybrid − semantic (recall@5) |
|---|---|---|
| 62 | ~81% of corpus (cap never binds) | −0.175 |
| 1,000 | ≤ 20% of corpus | −0.031 |
| 10,000 | ≤ 2% of corpus | **+0.026** |

At 62 memories the `LIMIT 200` cap never binds, the lexical leg ranks 81% of the
corpus, and fusion is swamped by noise. At 1,000 the deficit has nearly closed. At
10,000 the cap admits at most 2% of the corpus, the lexical leg becomes a genuine
precision signal rather than a noise generator, and **hybrid overtakes semantic on
recall@5 (0.6140 vs 0.5877) — gate G-Q4 passes at this scale.** Hybrid MRR remains
0.041 below semantic, so G-Q6 still fails, but narrowly rather than by 0.172.

**This substantially changes the interpretation of the gate failures.** The §13
fusion design is not wrong; it is **mis-tuned for small corpora**. The failure is
worst precisely at the corpus size the kernel proof mandates (≥ 50 memories) and
inverts by the time a store reaches the scale the ops gate targets (10K chunks).
Any real Folder Chief store spends its first weeks in the regime where hybrid
actively hurts, then crosses over.

Two honest caveats:

- Absolute recall falls for *every* mode as filler is added (semantic 0.811 →
  0.588), which is expected — 9,938 distractors is a much harder haystack — so the
  crossover is partly semantic degrading rather than fusion improving. Both
  effects are real and both follow from scale.
- The filler is synthetic. It is deliberately plausible engineering prose rather
  than lorem ipsum, so it competes for both legs, but it is not a substitute for
  10,000 genuine memories.

---

## 5. Operational metrics

### 5.1 Latency and footprint by scale

Single DB grown in place, so each scale is a strict superset of the previous.
All memories are single-chunk, so memory count = chunk count (verified: 10,000
memories / 10,000 chunks at the top scale).

| Corpus (= chunks) | DB size | warm ask median | warm ask p95 | engine median | cold start median | save median | save p95 | peak RSS |
|---|---|---|---|---|---|---|---|---|
| 62 | 0.27 MB | 477 ms | 514 ms | 465 ms | 452 ms | 515 ms | 539 ms | 300 MB |
| 1,000 | 2.97 MB | 465 ms | 481 ms | 454 ms | 427 ms | 529 ms | 562 ms | 301 MB |
| 10,000 | 28.99 MB | **532 ms** | 565 ms | 518 ms | **480 ms** | 522 ms | 549 ms | 303 MB |

Warm ask over 20 invocations per scale; cold start median of 5; save latency
cumulative over every save at that scale (n = 62 / 1,000 / 3,466).

**Everything is flat in corpus size.** Warm ask rises only 55 ms — 12% — across a
160× increase in corpus, and DB size grows linearly at roughly 3 KB per memory
(content + 384-dim f32 embedding + FTS index). Peak RSS is constant at ~300 MB.
None of these are corpus-driven costs; see §5.2.

### 5.2 The warm-latency gate fails for a structural reason

Isolating where the time goes (medians of 12 invocations, 62-memory corpus):

| Invocation | Wall time | Engine `elapsed_ms` |
|---|---|---|
| `ask --mode lexical` (no embedding needed) | **2.9 ms** | 1 |
| `ask --mode semantic` | 409.1 ms | 393 |
| `ask --mode hybrid` | 401.7 ms | 379 |
| `info` | 3.1 ms | — |
| `--version` (process spawn floor) | **1.5 ms** | — |

Process spawn costs 1.5 ms. Retrieval — FTS5 + brute-force cosine + RRF + JSON —
costs 1–3 ms. **Everything else, ~400 ms, is loading the granite model to embed a
single query string.** That cost is essentially constant in corpus size, which is
why latency is flat from 62 to 10,000 memories.

The consequence is that `architecture.md` §21.3's "warm ask < 250 ms" gate is
**unachievable by any process-per-invocation CLI that embeds the query**,
independent of retrieval quality or corpus size. It is not a performance
regression to be tuned away; it is a mismatch between the gate and the process
model. `--mode lexical` meets the gate with three orders of magnitude to spare,
which is the proof that retrieval is not the problem.

### 5.3 Cold start

Median 452 ms at 62 memories (5 spawns), 427 ms at 1,000, 480 ms at 10,000.
**Gate G-O1 (< 1.5 s) passes with ~3× headroom at every scale.** Note that for this CLI cold start and warm ask are
nearly the same measurement — there is no resident process to warm — which is
itself the observation behind §5.2.

### 5.4 Binary size — the gate does not apply to this build

The measured binary is **11.6 MB (12,125,184 bytes)**, but this is the
`model-sidecar` build, which loads weights from `SQLITE_MEM_MODEL_DIR` at runtime
and does **not** embed them. The §25 "≤ 150 MB" gate targets the
`--features embed-model` release build, which is a Sprint S6 artifact and was not
built here. Adding the f32 granite weights (~127 MB on disk) to 11.6 MB lands
near the 150 MB ceiling with limited margin; the f16 variant is the obvious lever
if S6 finds it tight. **Recorded as measured-but-not-applicable, not as a pass.**

### 5.5 Peak RSS

**~300 MB** (300,172 KB at 62 memories, 301,512 KB at 1,000), measured via
`/usr/bin/time -v` on a single `ask`. Flat across scales, consistent with the
finding that the model — not the corpus — dominates the process. Worth flagging
for §25 review: 300 MB resident for one CLI invocation is high for a tool an
agent may call repeatedly, and there is no memory gate currently specified.

---

## 6. Model ablation — DEFERRED, and why

`project-plan.md` S5 calls for a primary-vs-fallback model ablation (granite
vs bge-small-en-v1.5). **It was not run, and could not be from this sprint's
territory:** the shipped embedder implements granite only. There is no CLI flag,
environment variable, or feature gate that selects bge, and adding one requires
editing `src/`, which a Sprint S4 worker holds concurrently.

Partial closure of `architecture.md` §26.2 ("retrieval quality of a 47M model on
distilled-memory text"):

- The 47M granite model **is** by a wide margin the strongest single leg, and it
  carries the entire product claim on cross-worded queries.
- It is nonetheless **0.039 short of the recall@5 bar and 0.030 short of the MRR
  bar** on its own. A stronger embedder is a plausible route to closing that gap,
  and S1 already proved bge-small meets the same parity standard.

Running the bge (or arctic-embed-m-int8) arm is a prerequisite for a fully
informed G2 decision and needs a small, well-scoped `src/embed` change first.
Recommended as the first task of the next sprint.

---

## 7. Token economy (reported, not gated)

Tokens estimated as `ceil(chars / 4)`. Baseline is a **full-corpus scan** — an AI
with no index reading every memory-bearing document. This is deliberately
conservative: it assumes a perfect oracle that reads each document exactly once,
whereas the §3 scenario ("recursive tree exploration") also pays for directory
listings, re-reads, and files that turn out to be irrelevant.

| Corpus | Full-scan tokens | `ask --k 5` tokens | Reduction |
|---|---|---|---|
| 62 memories (measured) | 3,757 | 725 | **5.2×** |
| 1,000 memories (projected) | 55,342 | 725 | **76×** |
| 10,000 memories (projected) | 553,185 | 725 | **763×** |

Measured over 10 representative queries (every 4th query across the id-sorted
set, covering all four categories and both holdout states): median 725 tokens for
the full JSON envelope, 235 tokens for the memory content alone. Variance is
negligible (696–750) because output size is a function of `k`, not of the query.

**The shape of the result is the point.** `ask` output is O(k); a scan is
O(corpus). At 10,000 memories the full scan (553K tokens) exceeds most context
windows outright, so the comparison stops being a cost saving and becomes a
feasibility difference. Note the honest caveat: the 5.2× figure at the benchmark's
own 62-memory scale is unimpressive, and a real Folder Chief store only pays off
once it is well past a few hundred memories.

---

## 8. Binary defects found

**None.** Across roughly 11,000 `save` and 1,200+ `ask` invocations spanning
dataset authoring, the official run, the scale control, and the ops sweep:

- no crashes, panics, or undocumented exit codes;
- stdout was a single parseable JSON document on every invocation;
- output was byte-stable across repeated runs — the scale-62 quality re-run in
  `ops.json` reproduces the official run's aggregates to all five decimals
  (hybrid 0.63596 / 0.49737 both times), and a repeated `ask` is byte-identical
  once `elapsed_ms` is excluded, as §12's determinism contract requires.

Targeted adversarial probes, all clean:

| Probe | Result |
|---|---|
| FTS5 metacharacters `NEAR("a" OR b) AND * "` | exit 0, `ok: true` |
| Unicode + emoji `café naïve 日本語 🙂` | exit 0, `ok: true` |
| SQL-injection shape `'; DROP TABLE memories; --` | exit 0, `ok: true` |
| Stopwords only `the and of` | exit 0, `ok: true` |
| 400-token repeated query | exit 0, `ok: true` |
| Punctuation only `?!.,;:` | exit 0, `ok: true` |
| Empty query | exit **3** (documented) |
| `--k 99` (out of 1–50 range) | exit **2** (documented) |
| Malformed `--where bogus` | exit **2** (documented) |

The hybrid-underperforms-semantic finding is emphatically **not** an
implementation defect. `src/ask.rs` and `src/rank.rs` implement §13 faithfully.

Two **documentation** defects are worth recording:

1. **§12's worked example is unreachable at corpus scale.** It shows q01 returning
   m001 at rank 1 with `ranks: {lexical: 4, semantic: 1}`. On the 62-memory corpus
   the true ranks are lexical 12, semantic 14, and m001 is not returned at k=5 in
   any mode. The example is fine as a *schema* illustration but reads as a quality
   claim, and it is not one. See §8.1.
2. **§21.3's "warm ask < 250 ms" gate does not distinguish retrieval time from
   model-load time**, making it unachievable as written (§5.2). The gate needs
   restating against a defined measurement point.

Per the standing rule that divergence without amendment is a review failure, both
warrant an `architecture.md` amendment and changelog line — a supervisor action,
not a worker one.

### 8.1 The query that fails in every mode

q01, the mandated kernel query, is retrieved by neither hybrid, lexical, nor
semantic. **This is not a retrieval bug.** m001 names a product (*Mastra*) and a
failure mode (*suspend/resume durability*); it contains no token and no concept
resembling "agent framework". Knowing that Mastra **is** an agent framework is
world knowledge held by the calling LLM, not by a 47M-parameter embedding model.
The semantic leg places m001 at rank 14 of 62, behind three memories that
genuinely do discuss agents and frameworks (m004, m009, m012) — defensible
behaviour.

It is retained at full weight (cost ≈ 2.6 points of recall@5). It is an honest
illustration of the ceiling of embedding-only retrieval, and deleting the one
query that exposes a limitation is exactly the massaging the dataset design
document exists to prevent.

---

## 9. Recommendations for the G2 decision

Ordered by cost. All are Lee's calls, not a worker's.

1. **Fix the small-corpus fusion regime — do not redesign fusion.** §4 shows the
   §13 design already works at 10K; only small stores are broken, and every user's
   store starts small. The cheapest targeted fix is to **scale the lexical leg's
   candidate cap to the corpus** rather than fixing it at 200 — e.g.
   `min(200, ceil(0.05 × allowed_chunks))` — so the leg contributes its top few
   percent at every size, which is the property that makes it work at 10K.
   Alternatives, in increasing order of change: (a) require a minimum count of
   matched *non-stopword* query terms before a document enters the lexical ranking
   (a change to `build_fts5_query` alone); (b) weight the legs unequally in the RRF
   sum (`w_lex/(60+r)`, `w_lex < 1`); (c) admit only lexical results clearing a
   BM25 quality threshold. **Any candidate fix must be re-measured at 62, 1,000
   and 10,000** — a change that helps small corpora could easily undo the 10K
   crossover, and only the full sweep would show it.
2. **Re-run the model ablation** (§6) before concluding the embedder is adequate —
   semantic-only misses both bars by only 0.03–0.04, which a better embedder
   plausibly closes.
3. **Decide what the 250 ms warm-ask gate means** for a CLI paying model load per
   invocation (§5.2). Either restate it against engine time excluding model
   initialization, or accept that meeting it requires a persistent-process or
   daemon mode — which §23 currently defers.
4. **Do not change the default mode on this evidence.** Semantic-only wins at 62
   memories, ties by 1,000, and *loses* to hybrid on recall@5 at 10,000 (§4). It
   also loses q35 outright. Making `semantic` the default would optimise for the
   benchmark's corpus size and pessimise the scale real stores grow into. Fixing
   the small-corpus regime is far better supported by the data than abandoning
   fusion.
5. **Amend `architecture.md`** for the two documentation defects in §8.

---

## 10. Reproducing this report

All commands from the repository root. The benchmark drives the release binary as
a black box through its documented CLI/JSON contract; **nothing here ships with
the product**, which remains a single self-contained Rust binary with no Python.

```bash
# 0. Build the binary under test (release profile, default model-sidecar feature)
cargo build --release
export SQLITE_MEM_MODEL_DIR=/path/to/spike/embed-parity/models/granite
BIN=target/release/sqlite-mem

# 1. Verify the metric math (hand-computed fixtures, exact assertions)
python3 bench/run_bench.py --selftest

# 2. Full retrieval benchmark -> bench/results/<timestamp>/{results.json,summary.md}
python3 bench/run_bench.py --bin "$BIN"

# 3. Holdout subset only (the blind-authored 21%)
python3 bench/run_bench.py --bin "$BIN" --holdout-only

# 4. Ops metrics + retrieval-at-scale control (long: ~75 min, 10K sequential saves)
python3 bench/ops_bench.py --bin "$BIN" \
    --scratch /tmp/sqlite-mem-ops --out /tmp/sqlite-mem-ops-results \
    --scales 62,1000,10000 --repeats 20

# 5. Token economy (needs a DB already loaded with the golden corpus)
python3 bench/run_bench.py --bin "$BIN" --scratch /tmp/te --keep-db
python3 bench/token_economy.py --bin "$BIN" --db /tmp/te/bench.db --n 10
```

**Determinism.** The harness sorts all output, rounds to 5 decimals, and uses a
fixed filler seed, so reruns against the same binary and corpus produce identical
metrics. Only latency figures vary between runs.

**Artifacts:**

| Path | What |
|---|---|
| `bench/corpus/memories.jsonl` | 62 golden memories |
| `bench/corpus/queries.jsonl` | 38 queries with gold labels and holdout flags |
| `bench/corpus/DESIGN.md` | dataset rationale, category counts, holdout honesty note |
| `bench/run_bench.py` | frozen harness — sha256 `6b0d6e7139da660432ec3d05c793f43aa3620f337f5dc64c81ed145ae085218c` |
| `bench/ops_bench.py` | ops metrics + scale control |
| `bench/token_economy.py` | token-economy estimate |
| `bench/results/20260831T214806Z/results.json` | official 38-query run, per-query rows |
| `bench/results/20260831T214806Z/summary.md` | generated summary tables |
| `bench/results/20260831T214806Z/ops-62-1000.json` | ops + scale control, 62 and 1,000 |
| `bench/results/20260831T214806Z/ops-10000.json` | ops + scale control, 10,000 |
| `bench/results/20260831T214806Z/token-economy.json` | token-economy raw measurement |
---

## S5b — Tuned results (D016.1 / D016.2)

**Date:** 2026-08-31 · Same binary/model/host as §0 above, rebuilt from this
sprint's `src/ask.rs` changes (document-frequency query-token filtering +
corpus-scaled lexical cap, per `decisions.md` D016.1). This section is
appended, not a rewrite -- §1-§9 above remain the honest pre-fix record.

### S5b.1 What changed

`src/ask.rs`:

1. **Document-frequency filtering.** Before assembling the lexical leg's
   FTS5 `OR` query, each candidate token's document frequency is measured
   with one indexed `COUNT(*) ... MATCH` against the current `--where`-
   filtered scope. A token matching more than `DF_FILTER_FRACTION` (50%) of
   that scope is dropped from the `OR` set. **Absolute floor:** a token is
   never dropped while its raw document frequency is ≤ `DF_ABSOLUTE_FLOOR`
   (2) chunks, regardless of what fraction of a *tiny* corpus that is --
   not one of D016.1's three named tunables, but a necessary correction:
   the percentage rule alone always drops every token once the allowed
   scope is a handful of chunks (any token appearing in 1-2 of them is ≥
   any fraction in [0.25, 0.6]), which would make `--mode lexical`
   permanently nonfunctional on the small stores D016.1 itself calls out
   ("every user's store starts small"). Verified to change zero
   measurements at 62/1K/10K (see S5b.3). If every token is dropped, the
   lexical leg contributes nothing for that query and never falls back to
   the unfiltered query.
2. **Corpus-scaled candidate cap.** The old fixed `LIMIT 200` is now
   `min(200, max(4*k, ceil(allowed_chunks/10)))`, computed per query against
   the same `--where`-filtered scope as (1).
3. **Performance fix (D016.2), found during latency verification, not
   originally planned:** computing per-token document frequency by joining
   `chunks_fts` through `chunks.memory_id = ask_allowed.id` (a `TEXT`
   equality) is expensive once a token's match set is large -- on the
   10,000-chunk scale DB, a single common-token `COUNT` cost 5-12ms, and an
   all-but-one-token-common synthetic query pushed the whole `ask` past
   80ms, over the D016.2 50ms retrieval-only budget. Fix: materialize
   `temp.ask_allowed_chunk_rowids` (the allowed set's integer
   `chunks.rowid`s) once per `ask` call, and join both the DF `COUNT`
   queries and the lexical leg's own `MATCH` query against that instead of
   re-deriving the `TEXT`-keyed join every time. Measured effect on the
   worst case found (see S5b.4): median engine time on an all-stopword nine
   -token query at 10K chunks dropped from 56ms to 31ms.

LoC (`git diff --numstat`): `src/ask.rs` +384/-30 (implementation, doc
comments, and 11 new unit tests: DF filtering incl. the all-tokens-dropped
path, the absolute floor, the corpus-scaled cap at floor/mid-range/ceiling,
and rowid-materialization idempotency). `tests/ask_contract.rs` +24/-0 (one
test's fixture diluted with 45 decoy memories so DF filtering doesn't zero
its single-token query at 5/5 chunks -- 100% document frequency; one
assertion's comment updated to explain why the absolute floor keeps
`ranks.lexical` populated on a 1-memory DB). No dependency, verb, or flag
changes. `candidates` stat semantics documented in `Stats`'s doc comment
(§ below) -- it is no longer "everything either leg
matched"; DF filtering and the cap can each independently shrink it.

### S5b.2 Tuning sweep (62-memory corpus, fast -- full 38-query bench per
config)

The three tunables were swept within D016.1's authorized bounds (fraction
[0.25, 0.6], cap divisor [5, 20], cap floor multiplier [2k, 6k]). Baseline
(pre-fix, §3.1 above) and semantic (untouched by this leg's changes,
included as the ceiling) are repeated for reference.

| Config (fraction / divisor / floor×k) | hybrid recall@5 | hybrid MRR | note |
|---|---|---|---|
| **pre-fix baseline** (unfiltered, fixed cap 200) | 0.6360 | 0.4974 | §3.1 |
| 0.25 / 20 / 2 (max filtering, min cap) | 0.5570 | 0.5097 | worst recall@5 tried |
| 0.35 / 10 / 4 | 0.5965 | 0.5158 | ~ties default |
| **0.50 / 10 / 4 (chosen -- spec's suggested defaults)** | **0.5965** | **0.5171** | |
| 0.50 / 5 / 6 (looser cap) | 0.5965 | 0.5171 | identical to default -- cap not binding at 62 in this range |
| 0.60 / 5 / 6 (loosest fraction, looser cap) | 0.5965 | 0.5171 | identical to default |
| 0.60 / cap≈200 (diagnostic: cap effectively disabled) | 0.5965 | 0.5171 | isolates the effect to DF filtering alone, not the cap |
| 0.40 / 20 / 2 (tight cap) | 0.4912 | 0.4702 | tightening the cap alone *hurts* |
| 0.60 / 20 / 2 (loosest fraction + tight cap) | 0.4912 | 0.4702 | same -- confirms the cap dominates once tight, regardless of fraction |
| **semantic (unaffected)** | 0.8114 | 0.6697 | ceiling |

**Findings from the sweep:**

- **The candidate cap does not bind at 62 memories anywhere in its
  authorized range once the divisor is ≥ 5 and the floor multiplier is
  ≤ 6** -- five different cap settings from `max(20, 7)=20` up to an
  effectively-disabled `≈200` all produced the *identical* 5-decimal result
  (0.59649 / 0.51711). At this scale the number of tokens surviving DF
  filtering, not the `LIMIT`, determines how many candidates the lexical
  leg contributes. **Tightening the cap only ever hurts** (0.4912 vs 0.5965)
  -- it excludes real candidates from fusion without discriminating between
  helpful and noisy ones.
- **The DF fraction is the only lever that moves the number at 62,** and
  more aggressive filtering (0.25) is *worse* than looser filtering (0.5-0.6),
  not better. Fractions ≥ 0.5 all converge on the same result.
- **No configuration in the authorized sweep reaches hybrid ≥ semantic at
  62 memories**, and the best configuration found (0.5965 recall@5) is
  *below* the pre-fix baseline's recall@5 (0.6360), despite improving MRR
  (0.5171 vs 0.4974) and recall@1 (0.4298 vs 0.3772).

### S5b.3 Why DF filtering does not close the gap at 62: the RRF floor-removal
effect

This is the honest mechanistic answer to "why doesn't the specified fix
work here," established by isolating the cap (S5b.2's diagnostic row) so
only DF filtering is varied.

RRF fusion is **additive**, not merely a rank agreement signal: a document
is scored `score = 1/(60+rank_lex) + 1/(60+rank_sem)` and either term is
omitted if the document is absent from that leg. Before this fix, the
unfiltered lexical `OR` query matched a mean of 81% of the 62-memory corpus
(§3.4) -- so nearly *every* candidate, including the true gold document,
received *some* lexical-leg contribution, however weak. That contribution
was close to uniform across candidates (a near-flat "floor"), so it barely
disturbed the *relative* ordering the semantic leg already got right --
except in the specific cases (§3.3's 13 "hybrid worse than semantic" queries)
where a wrong document's lexical rank happened to be strong enough on
stopword mass alone to add more than its fair share.

DF filtering makes the lexical leg **selective**: only genuine, low-frequency
content-word matches survive. This removes the uniform floor -- most
documents, gold ones included, now get *no* lexical contribution at all,
while a minority (not necessarily the correct ones) still get a real boost
from incidentally sharing a low-frequency content word with the query (the
report's own q17 worked example, m012 vs m033, §3.4, is exactly this
mechanism and is **not stopword-driven** -- `rules`/`govern` are ordinary
words with a low corpus document frequency, so DF filtering does not touch
them). Removing the floor without removing that second effect trades a
small, mostly-harmless perturbation for a smaller number of larger,
still-harmful ones -- net negative for recall@5 in this dataset, even
though it improves recall@1/MRR (a document that *does* get promoted this
way is more often promoted all the way to rank 1, which is why MRR and
recall@1 both improve while recall@5 does not).

**This is a property of RRF's additive fusion, not a bug in the DF filter
or the cap implementation** -- confirmed by the diagnostic row in S5b.2,
where disabling the cap entirely (restoring the old effectively-unbounded
`LIMIT`) while DF filtering stays active still produces the same 0.5965,
not the pre-fix 0.6360. D016.1's two specified mechanisms (percentage-based
DF filtering, corpus-scaled cap) target *document frequency*, which is not
the dimension the second effect operates on; neither mechanism, at any
setting inside the authorized bounds, can distinguish "shares a rare word
with the query but is the wrong document" from "shares a rare word with the
query and is the right document." A fix for that would need to change *how*
legs are combined (e.g. a discriminating leg weight, or a BM25-score floor
rather than a document-frequency one) -- both are explicitly out of this
sprint's authorized scope (D016.1 names DF filtering and the cap only;
`architecture.md` §9's recommendation (b)/(c) alternatives were not
authorized).

### S5b.4 Latency (D016.2)

Retrieval-only (`ask --mode lexical`, no embedder load), 10,000-chunk scale
DB (reused from the S5 agent's scratch -- `info` confirmed 10,000 active
memories / 10,000 chunks / 28.99 MB before use, unchanged by this sprint's
query-side fix):

| Query set | n | engine `elapsed_ms` median | p95 | max |
|---|---|---|---|---|
| All 38 golden bench queries | 38 | 25 | 34 | 34 |
| Adversarial all-stopword synthetic (`"the and of a to what how we do"`, 9 tokens) -- before the rowid-materialization fix (S5b.1 item 3) | 20 reps | 56 | 75 | 75 |
| Same adversarial query -- after the fix | 20 reps | 31 | 40 | 40 |

**D016.2's retrieval-only < 50ms gate at 10K chunks: PASS**, with margin,
including the worst case found during testing (an intentionally
pathological all-near-stopword query). Wall-clock end-to-end (process spawn
+ retrieval, `--mode lexical` never loads the embedder) median ~23ms, p95
~29ms over 20 reps -- also comfortably inside D016.2's restated <1s
end-to-end warm-ask gate.

### S5b.5 Gate table, chosen configuration (fraction=0.5, divisor=10, floor×k=4)

| # | Gate | Required | Pre-fix (§1) | S5b (tuned) | Verdict |
|---|---|---|---|---|---|
| Required: hybrid recall@5 ≥ semantic (62) | ≥ 0.8114 | 0.6360 | 0.5965 | ❌ **still FAIL** -- see S5b.3 |
| Required: hybrid MRR ≥ semantic (62) | ≥ 0.6697 | 0.4974 | 0.5171 | ❌ **still FAIL** |
| Required: hybrid ≥ lexical, recall@5 (62) | ≥ 0.4123 | 0.6360 | 0.5965 | ✅ PASS |
| Required: hybrid ≥ lexical, MRR (62) | ≥ 0.2026 | 0.4974 | 0.5171 | ✅ PASS |
| Required: hybrid ≥ semantic preserved at 10K | hybrid > semantic | 0.6140 > 0.5877 | **0.6228 > 0.5877** | ✅ **PASS -- crossover preserved, margin slightly wider** |
| Required: determinism (byte-identical reruns) | pass | pass | pass (verified at 62 via unit test + manually at 10K, hybrid mode) | ✅ PASS |
| Required: retrieval-only latency < 50ms at 10K | < 50ms | 1-3ms at 62 memories only (§5.2); not measured pre-fix at 10K | 25ms median / 34ms max (38 real queries); 31ms median / 40ms max (worst synthetic case) | ✅ PASS |
| Target: hybrid recall@5 ≥ 0.85 (62) | ≥ 0.85 | 0.6360 | 0.5965 | ❌ FAIL |
| Target: hybrid MRR ≥ 0.70 (62) | ≥ 0.70 | 0.4974 | 0.5171 | ❌ FAIL |

**Holdout (8 blind queries), chosen configuration:**

| Mode | recall@5 | MRR |
|---|---|---|
| hybrid | 0.5000 | 0.4000 |
| lexical | 0.3750 | 0.1146 |
| semantic | 0.6250 | 0.5417 |

Identical to 5 decimals to the pre-fix holdout numbers in `corpus/DESIGN.md`
§4 -- the fix has no effect on this particular 8-query subset (none of its
queries happen to hit the DF-filtering-sensitive path differently), which
is itself evidence the fix's effect elsewhere is real and not overfitting
noise.

### S5b.6 Honest verdict

**D016.1's two specified mechanisms (DF filtering + corpus-scaled cap) do
not achieve hybrid ≥ semantic at 62 memories, at any setting inside the
authorized tuning bounds**, and the 0.85/0.70 target (D016.3's TARGET,
not REQUIRED OUTCOME) is not reached either. The mechanism is understood
and reproducible (S5b.3): RRF's additive fusion means a selective lexical
leg removes a mostly-harmless "everyone gets ranked" floor while leaving
intact the harder failure mode (a wrong document sharing a genuine,
low-frequency content word with the query) that DF filtering cannot see,
because that failure is not a document-frequency problem. What *is*
achieved: hybrid ≥ lexical everywhere at all three scales (required, and
was already true pre-fix), MRR and recall@1 both improve over the pre-fix
baseline at 62 memories (0.5171 vs 0.4974, 0.4298 vs 0.3772), **the 10K
crossover requirement is preserved and its margin is not worse** (0.6228 vs
0.5877, +0.0351, against the pre-fix +0.0263 -- S5b.7), determinism holds,
and the D016.2 latency budget is met with margin (including a real
regression found and fixed along the way, S5b.1 item 3 / S5b.4).

Per D016.3, gate recalibration to the best-achieved tuned numbers is
pre-authorized at the supervisor level and is **not applied here** -- this
section reports the tuned numbers and the sweep evidence for that decision,
it does not make it.

### S5b.7 Full-scale sweep: 62 / 1,000 / 10,000 chunks, chosen configuration

Same methodology as §4 above (`bench/ops_bench.py`, single DB grown in
place, deterministic synthetic filler seed 20260831, golden 62 loaded
first): re-run end to end against this sprint's binary, all three scales in
one continuous pass.

| Corpus | Mode | recall@5 | MRR | hybrid − semantic (recall@5) |
|---|---|---|---|---|
| 62 | hybrid | 0.5965 | 0.5171 | **−0.2149** |
| 62 | lexical | 0.4123 | 0.2026 | |
| 62 | semantic | 0.8114 | 0.6697 | |
| 1,000 | hybrid | 0.5965 | 0.4987 | **−0.0570** |
| 1,000 | lexical | 0.3290 | 0.2149 | |
| 1,000 | semantic | 0.6535 | 0.6009 | |
| 10,000 | **hybrid** | **0.6228** | 0.5250 | **+0.0351** |
| 10,000 | lexical | 0.3290 | 0.2004 | |
| 10,000 | semantic | 0.5877 | 0.5658 | |

Compared to the pre-fix §4 numbers at the same scales:

| Corpus | Mode | pre-fix recall@5 | S5b recall@5 | pre-fix MRR | S5b MRR |
|---|---|---|---|---|---|
| 62 | hybrid | 0.6360 | 0.5965 (worse) | 0.4974 | 0.5171 (better) |
| 1,000 | hybrid | 0.6228 | 0.5965 (worse) | 0.5040 | 0.4987 (~same) |
| 10,000 | hybrid | 0.6140 | 0.6228 (better) | 0.5250 | 0.5250 (same) |
| 62/1K/10K | semantic, lexical | -- | unchanged to ~3 decimals | -- | unchanged (the semantic leg is untouched by this fix; small lexical deltas are DF filtering doing *something* even where the cap dominates) |

**The picture that emerges across the sweep, honestly stated:**

- **The 10K crossover is not just preserved, it strengthens slightly**
  (hybrid − semantic recall@5 goes from +0.0263 pre-fix to +0.0351 post-fix,
  and hybrid recall@5 itself improves 0.6140 → 0.6228) -- this is the one
  scale where DF filtering's premise (stopword mass matters less once the
  corpus is large and content-word matches are genuinely selective) holds,
  because at 10K chunks the corpus-scaled cap (`ceil(10000/10)=1000`,
  clipped to the 200 ceiling) was already the dominant mechanism pre-fix
  (§4's own finding), and DF filtering is now acting on top of an already-
  working regime rather than trying to fix a broken one.
- **62 and 1,000 both get *worse* on recall@5 relative to the pre-fix
  baseline** (−0.0396 and −0.0263 respectively), for the reason established
  in S5b.3: removing the near-uniform "everyone gets some lexical rank"
  floor that stopword flooding used to provide costs more at these scales
  (where that floor was doing real, if crude, work smoothing over the
  semantic leg's own misses) than it gains by suppressing egregious
  stopword-driven promotions.
- **MRR moves in the fix's favor at 62** (+0.0197) even though recall@5
  moves against it -- consistent with S5b.3's account: the failure mode DF
  filtering does not fix (a wrong document promoted by sharing a real, rare
  content word) more often promotes that document all the way to rank 1
  than it does to ranks 2-5, so MRR (which weights rank 1 heavily) and
  recall@1 improve while recall@5 (which does not distinguish rank 1 from
  rank 5) does not.
- **1,000 sits in between**, closer to 62's pattern than 10K's, consistent
  with §4's original finding that 1,000 is a transitional scale where the
  cap is starting to bind but has not yet become the dominant mechanism.

### S5b.8 Test suite and static checks

- `cargo test --no-default-features --features test-support --all-targets`:
  **174 passed, 0 failed, 1 ignored** (163 pre-S5b + 11 new unit tests in
  `src/ask.rs`'s `s5b_tuning_tests` module -- DF filtering incl. the
  all-tokens-dropped path, the absolute-floor exemption, the corpus-scaled
  cap at its floor/mid-range/ceiling, and rowid-materialization idempotency
  -- + 1 previously-existing test diluted in `tests/ask_contract.rs` so its
  single query token isn't at 100% document frequency in its 5-chunk
  fixture; 1 ignored, pre-existing and unrelated).
- `cargo clippy --all-targets -- -D warnings` (default `model-sidecar`),
  `--no-default-features --features test-support`, and
  `--no-default-features --features embed-model`: all three **clean**,
  matching CI's three clippy jobs.
- `cargo fmt -- --check`: **clean**.
- Determinism: the existing
  `determinism_two_runs_are_byte_identical_except_elapsed_ms` unit test
  passes, and a manual double-run of `ask --mode hybrid` against the
  10,000-chunk scale DB was byte-identical once `elapsed_ms` was excluded.

### S5b.9 Reproducing this section

```bash
# 0. Build (same as §10 above)
cargo build --release
export SQLITE_MEM_MODEL_DIR=/path/to/spike/embed-parity/models/granite
BIN=target/release/sqlite-mem

# 1. Unit tests (incl. the 11 new S5b tests) + clippy + fmt
cargo test --no-default-features --features test-support --all-targets
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features --features test-support -- -D warnings
cargo clippy --no-default-features --features embed-model -- -D warnings
cargo fmt -- --check

# 2. Main 62-query bench + holdout-only (fast, ~10s each)
python3 bench/run_bench.py --bin "$BIN"
python3 bench/run_bench.py --bin "$BIN" --holdout-only

# 3. Full 62/1,000/10,000 scale sweep with quality re-run (long: ~90 min,
#    9,938 sequential filler saves at ~0.5s/save -- this is save/model-load
#    cost, not a retrieval regression; see §5.2/S5b.4)
python3 bench/ops_bench.py --bin "$BIN" \
    --scratch /tmp/sqlite-mem-s5b-ops --out /tmp/sqlite-mem-s5b-ops-results \
    --scales 62,1000,10000 --repeats 20

# 4. D016.2 retrieval-only latency at 10K, isolated (needs a 10,000-chunk
#    DB, e.g. the one step 3 produces, or ops.db from a prior scale run)
for i in $(seq 1 20); do
  "$BIN" ask --db /path/to/10k.db --mode lexical \
    --query "what is the retry policy for idempotent writes" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['stats']['elapsed_ms'])"
done
```

**Artifacts (this section):**

| Path | What |
|---|---|
| `bench/results/20260901T014427Z-S5b/results.json` | S5b official 38-query run, per-query rows |
| `bench/results/20260901T014427Z-S5b/summary.md` | generated summary tables (S5b.5's 62-scale numbers) |
| `bench/results/20260901T014427Z-S5b/holdout-results.json`, `holdout-summary.md` | S5b `--holdout-only` run |
| `bench/results/20260901T014427Z-S5b/ops-62-1000-10000.json` | S5b.7's full scale sweep (ops metrics + quality re-run, all three scales, one continuous `ops_bench.py` run) |

---

## S5c — Supervisor ruling: hybrid-mode small-corpus deactivation (D016.3)

**Date:** 2026-08-31 (continuation of S5b). Same binary/model/host, rebuilt
from one further `src/ask.rs` change.

### S5c.1 The ruling

S5b's sweep (§S5b.2/S5b.6/S5b.7) showed no DF-fraction/cap configuration
inside D016.1's authorized tuning bounds got `--mode hybrid` to score at or
above `--mode semantic` on the same corpus at 62 or 1,000 allowed chunks.
Recalibrating the *gate* to that shortfall (D016.3) while the shipped
binary's own `--mode semantic` measures higher on the identical corpus
would leave `architecture.md` §24's "default mode must be >= each pure
mode" invariant broken by construction — a gate number cannot fix an
invariant the binary itself violates.

Ruling applied: the corpus-scaled lexical cap's correct value in the
small-corpus regime is **zero**. `src/ask.rs` gains one more mechanism,
`effective_lexical_cap`, layered on top of S5b's tuned configuration:

- **Below `LEXICAL_ACTIVATION_CHUNKS` (4,096) allowed chunks, `--mode
  hybrid`'s lexical leg does not run at all.** The DF-filtering step and
  the FTS5 `MATCH` query are both skipped (not merely capped at 0 and
  discarded) once `effective_lexical_cap` returns 0, so no candidates enter
  fusion from that leg. RRF fusion over a single populated leg reduces
  algebraically to that leg's own ranking, so hybrid becomes byte-identical
  to `--mode semantic`'s ranking on the same query -- **verified**, not
  assumed (S5c.2). `ranks.lexical` is simply absent, the same JSON shape
  already used whenever a leg didn't run (e.g. `--mode semantic` itself).
- **At or above the threshold, S5b's tuned configuration (fraction=0.5,
  divisor=10, floor×k=4, `DF_ABSOLUTE_FLOOR`=2, rowid-materialized DF/cap
  queries) runs completely unchanged** -- verified byte-identical to the
  pre-ruling S5b measurement at 10K (S5c.2).
- The threshold (4,096) sits between the S5b sweep's two measured data
  points: 1,000 allowed chunks, where hybrid still lost to semantic every
  configuration tried, and 10,000, where it won and the crossover was
  verified to hold and slightly widen. It is placed conservative toward
  keeping the lexical leg *off* -- closer to 10,000 than to 1,000 -- since
  only the 10,000-chunk point is directly verified to work; nothing in the
  1,000-10,000 range is measured, so the threshold is a judgment call
  within that gap, not an interpolated optimum.
- **`--mode lexical` and `--mode semantic` are unaffected.** Both remain
  explicit, single-leg requests that run regardless of corpus size, exactly
  as before this change. No new flags.

`src/ask.rs` / `tests/ask_contract.rs` are uncommitted throughout S5b and
S5c, so there is no S5b-only commit boundary to diff against; cumulative
`git diff --numstat` against the pre-S5b `HEAD` (`b228177`) is `src/ask.rs`
+501/−33, `tests/ask_contract.rs` +29/−1. This step's own increment on top
of the S5b state: the `LEXICAL_ACTIVATION_CHUNKS` constant and its doc
comment, `effective_lexical_cap`, the `run()` call-site change (compute the
cap once, skip DF filtering and the `MATCH` query entirely when it is 0, no
longer relying on `LIMIT 0`), 3 new unit tests (both sides of the
threshold, and that `--mode lexical` ignores it), plus module- and
`Stats`-level doc updates -- roughly 100 lines. `tests/ask_contract.rs`: 1
assertion flipped (`ranks.lexical` is now absent, not present, for a
hybrid-mode query against a 1-memory DB) with its comment rewritten to
explain why.

### S5c.2 Verification: hybrid == semantic below threshold, unchanged at/above

**62-memory corpus, full 38-query bench, unfiltered:**

| Mode | recall@1 | recall@5 | MRR | nDCG@5 |
|---|---|---|---|---|
| **hybrid** | 0.5219 | 0.8114 | 0.6697 | 0.6864 |
| lexical | 0.0790 | 0.4123 | 0.2026 | 0.2510 |
| **semantic** | 0.5219 | 0.8114 | 0.6697 | 0.6864 |

**Hybrid and semantic are identical to all 5 decimals** -- as expected,
since `effective_lexical_cap` returns 0 for every query on this 62-chunk
corpus (well below 4,096) in hybrid mode. Verified at the ranking level,
not just the aggregate metric: every one of the 46 hybrid/semantic query×
cell pairs in the run (unfiltered + `--where`-filtered metadata-scoped
cells) has a **byte-identical ranked-id list and candidate count** between
the two modes -- checked programmatically against `results.json`'s
`per_query` rows, not eyeballed from the aggregate table, since matching
aggregates alone wouldn't rule out different per-query rankings that happen
to average out the same.

Gate verdicts (unfiltered):

| Gate | Required | Value | Verdict |
|---|---|---|---|
| hybrid recall@5 ≥ semantic | ≥ 0.8114 | 0.8114 | ✅ PASS (equal) |
| hybrid MRR ≥ semantic | ≥ 0.6697 | 0.6697 | ✅ PASS (equal) |
| hybrid recall@5 ≥ lexical | ≥ 0.4123 | 0.8114 | ✅ PASS |
| hybrid MRR ≥ lexical MRR | ≥ 0.2026 | 0.6697 | ✅ PASS |
| hybrid recall@5 ≥ 0.85 (target) | ≥ 0.85 | 0.8114 | ❌ FAIL (semantic's own ceiling; unaffected by this change) |
| hybrid MRR ≥ 0.70 (target) | ≥ 0.70 | 0.6697 | ❌ FAIL (semantic's own ceiling) |

**Holdout (8 blind queries):**

| Mode | recall@1 | recall@5 | MRR | nDCG@5 |
|---|---|---|---|---|
| **hybrid** | 0.5000 | 0.6250 | 0.5417 | 0.5625 |
| lexical | 0.0000 | 0.3750 | 0.1146 | 0.1788 |
| **semantic** | 0.5000 | 0.6250 | 0.5417 | 0.5625 |

Identical to 4 decimals -- same deactivation applies (holdout corpus is the
same 62-memory DB). `hybrid recall@5 ≥ semantic` and `hybrid MRR ≥
semantic` both **PASS (equal)** on the holdout gate table too.

Filtered (metadata-scoped, `--where`-narrowed) cells also come out
hybrid == semantic exactly (0.7500/0.7500 both metrics, main run) -- the
`--where`-filtered allowed scope for any single project in this corpus
(10-14 memories) is still far below 4,096, so the same deactivation
applies there too, consistent with `effective_lexical_cap` and `lexical_
cap` both being computed against the *filtered* allowed scope, not the
whole DB.

**10,000-chunk scale, spot-check against the reused S5b/S5 scale DB**
(`info` confirmed 10,000 active memories / 10,000 chunks / 28.99 MB,
unchanged): re-ran all 38 queries with the ruling-updated binary, using an
id mapping recovered from the DB's own ascending-ULID creation order
(verified against `memories.jsonl` content, not assumed) since 10,000 >
`LEXICAL_ACTIVATION_CHUNKS`, so S5b's tuned configuration should apply
completely unchanged:

| Mode | recall@1 | recall@5 | MRR | nDCG@5 |
|---|---|---|---|---|
| hybrid | 0.40351 | 0.62281 | 0.52500 | 0.53080 |
| lexical | 0.09211 | 0.32895 | 0.20044 | 0.22593 |
| semantic | 0.46930 | 0.58772 | 0.56579 | 0.54807 |

**Byte-identical to 5 decimals to the pre-ruling S5b tuned measurement at
10K** (`bench/results/20260901T014427Z-S5b/ops-62-1000-10000.json`'s
`memories: 10000` entry: hybrid recall@5 0.62281 / MRR 0.525). The 10K
hybrid ≥ semantic crossover (0.62281 > 0.58772) is unaffected by this
change, exactly as intended -- this scale is at/above the activation
threshold, so nothing about its retrieval path changed.

### S5c.3 Determinism

- The existing `determinism_two_runs_are_byte_identical_except_elapsed_ms`
  unit test passes.
- Manual double-run check, 10,000-chunk DB, `--mode hybrid`: byte-identical
  (`elapsed_ms` excluded).
- Manual double-run check, fresh 1-memory DB, `--mode hybrid` (the
  deactivated-leg path): byte-identical (`elapsed_ms` excluded);
  `ranks.lexical` confirmed absent from the JSON in both runs.

### S5c.4 Test suite and static checks (post-ruling)

- `cargo test --no-default-features --features test-support --all-targets`:
  **177 passed, 0 failed, 1 ignored** (174 post-S5b + 3 new unit tests:
  `hybrid_cap_is_zero_below_the_activation_threshold`,
  `hybrid_cap_matches_lexical_cap_at_and_above_the_activation_threshold`,
  `explicit_lexical_mode_ignores_the_activation_threshold_entirely`; 1
  existing `tests/ask_contract.rs` assertion updated -- `ranks.lexical` is
  now correctly asserted absent, not present, for a hybrid-mode query
  against a 1-memory DB).
- `cargo clippy --all-targets -- -D warnings` (default `model-sidecar`),
  `--no-default-features --features test-support`, and
  `--no-default-features --features embed-model`: all three **clean**.
- `cargo fmt -- --check`: **clean**.

### S5c.5 Final numbers table

| Scale | Mode | recall@5 | MRR | vs. required |
|---|---|---|---|---|
| 62 (main, 38q) | hybrid | 0.8114 | 0.6697 | = semantic, both gates PASS |
| 62 (main, 38q) | semantic | 0.8114 | 0.6697 | (ceiling, unaffected) |
| 62 (main, 38q) | lexical | 0.4123 | 0.2026 | hybrid ≥ lexical, PASS |
| 62 (holdout, 8q) | hybrid | 0.6250 | 0.5417 | = semantic, both gates PASS |
| 62 (holdout, 8q) | semantic | 0.6250 | 0.5417 | (ceiling) |
| 10,000 | hybrid | 0.6228 | 0.5250 | ≥ semantic, PASS (crossover preserved, unchanged from S5b) |
| 10,000 | semantic | 0.5877 | 0.5658 | |
| 10,000 | lexical | 0.3290 | 0.2004 | hybrid ≥ lexical, PASS |

**architecture.md §24's invariant now holds by construction at every
measured scale**: hybrid equals semantic exactly below the activation
threshold (so it can never score lower) and exceeds it at 10K (unchanged
from S5b). The 0.85/0.70 absolute target is still not met at 62 -- that
ceiling is semantic's own (0.8114/0.6697), untouched by any change made in
S5b or S5c, and D016.3's pre-authorized gate recalibration remains the
supervisor's call, not applied here.

**Artifacts (this section):**

| Path | What |
|---|---|
| `bench/results/20260901T015743Z-S5c/results.json` | S5c official 38-query run, per-query rows (hybrid == semantic verified programmatically, S5c.2) |
| `bench/results/20260901T015743Z-S5c/summary.md` | generated summary tables |
| `bench/results/20260901T015743Z-S5c/holdout-results.json`, `holdout-summary.md` | S5c `--holdout-only` run |
