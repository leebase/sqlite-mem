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
