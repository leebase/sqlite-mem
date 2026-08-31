# Golden dataset design — sqlite-mem kernel proof

**Sprint:** S5 · **Authored:** 2026-08-31 · **Corpus:** 62 memories, 38 queries

This document explains *why* the dataset looks the way it does, so that a later
reader can tell the difference between "the retriever got worse" and "somebody
made the benchmark easier". The dataset is the load-bearing part of Sprint S5:
gates computed over an easy corpus are decoration.

---

## 1. What the dataset is trying to prove

`architecture.md` §3 makes a specific claim about when an AI should call `ask`:

> when the AI suspects precedent ("have we hit this before?"), and **when it
> doesn't know the filename or wording** — the exact case where recursive tree
> exploration burns tokens.

"Doesn't know the wording" is the whole product. A memory store that only works
when the caller already guesses the right nouns is a `grep` with extra steps.
So the dataset is built to make *wording* useless and *meaning* necessary.

The design target, stated as a falsifiable property: **a lexical-only retriever
should do poorly on this corpus.** If `--mode lexical` scores near `--mode
hybrid`, the dataset has failed, regardless of what the gate table says.

Measured result: lexical-only reaches recall@5 = 0.41 and MRR = 0.19, against
semantic-only at 0.81 / 0.67. The dataset discriminates.

---

## 2. The memory corpus (`memories.jsonl`, 62 records)

### Shape

Each record is a **distilled statement**, not a document — one or two sentences,
median 30 words, max 47. This follows §3's "save the distilled statement (1–10
sentences), not raw file dumps". Every memory would survive the question "would
a competent engineer actually have written this down?"

Deliberately, almost every memory is a **single chunk**. That is the realistic
case (§11.3: "most distilled memories are one chunk") and it keeps the
chunk→memory collapse step from silently doing work the ranker should be doing.

### Spread

| Project | Domain | Count |
|---|---|---|
| `factory` | AI agent orchestration (the project the Mastra decision belongs to) | 14 |
| `atlas` | data ingestion / warehouse | 14 |
| `harbor` | payments and billing | 13 |
| `lantern` | observability | 11 |
| `quill` | design system / frontend | 10 |

Five fictional projects, exceeding the ≥ 4 requirement. Multiple projects matter
for two reasons: metadata-scoped queries need something to scope *away*, and
cross-project near-misses are the realistic failure mode (a privacy constraint
in `atlas` looks a lot like a card-data constraint in `harbor` — see q17).

| Kind | Count | What it captures |
|---|---|---|
| `decision` | 21 | a choice made, with its rationale |
| `constraint` | 13 | a rule the system must obey |
| `preference` | 10 | a stated way-we-like-to-do-things |
| `precedent` | 9 | "we have hit this before, and here is what happened" |
| `review` | 9 | a review outcome — accepted, accepted-with-reservation, or sent back |

All five kinds named in `project-plan.md` S5 and `architecture.md` §3 are
present. Metadata follows §3 conventions exactly: `project`, `kind`, `status`,
and `source` provenance pointing at a plausible Markdown anchor
(`decisions.md#D012`, `postmortems/2026-03-scheduler.md`, `runbook.md#sla`).
`source` is passed to the binary via `--source`, not as a `--meta` pair, so it
lands in the system provenance field where §12 puts it.

### Planted confusability

Realism alone does not make a corpus hard; *adjacent* memories do. Several
clusters were planted so that a retriever must discriminate within a topic
rather than merely find the topic:

- **Immutability cluster** — m015 (append-only partitions), m032 (refunds as new
  ledger entries), m037 (invoices immutable + credit note), m002 (replay from
  event log). Four different projects, one shared concept. Query q30 asks for
  all of the first three at once.
- **Retry / idempotency cluster** — m030 (idempotency key), m031 (double-charge
  postmortem), m006 (backoff without jitter), m003 (queue starvation).
- **Naming cluster** — m024 (`duration_ms`, units in column names) and m056
  (tokens named by role, `surface-raised` not `gray-100`). Both are `kind=preference`
  and both are "how we name things"; only one answers q21.
- **Accessibility cluster** — m054 (constraint) and m057 (a review that failed
  on focus restoration). Both are gold for q25.
- **"We tried it twice and reverted" cluster** — m009 (plugin ABI), m055
  (CSS-in-JS), m038 (partial captures, asked three times, refused each time).
  These share a rhetorical shape but nothing else, which punishes a retriever
  that keys on phrasing.

### The mandated record

The Mastra memory is present verbatim as required:

```
m001  "We rejected Mastra because suspend/resume durability violated the Factory invariants."
      project=factory  kind=decision  source=decisions.md#D012
```

---

## 3. The query set (`queries.jsonl`, 38 records)

### Category counts

| Category | Count | Holdout | Definition |
|---|---|---|---|
| cross-wording | 15 | 3 | paraphrase / synonym / conceptual restatement of the gold memory |
| adversarial | 8 | 2 | built so a lexical-only retriever should plausibly score zero |
| metadata-scoped | 8 | 2 | carries a `--where` filter; evaluated both filtered and unfiltered |
| precedent | 7 | 1 | "have we dealt with X before?" — the §3 precedent-seeking case |
| **total** | **38** | **8 (21%)** | |

Four queries have **multiple** relevant memories (q08, q24, q25, q30) — above
the ≥ 3 requirement. q30 has three golds spanning three projects. 40 distinct
memories are referenced as gold across the set, so the corpus is not a thin
wrapper around a handful of answers.

### Cross-wording, measured rather than asserted

Content-word overlap between each query and its gold memory (stopwords and
words ≤ 3 characters removed):

- **27 of 38 queries (71%) share *zero* content words with any of their gold memories.**
- Mean overlap across the whole set: **0.32 words per query**.
- The single worst offender is q21 at two shared words (`field`, `unit`); every
  other query shares at most one.

That is the property the whole benchmark rests on, so it is checked
arithmetically rather than eyeballed.

### Query #1, as mandated

```
q01  "Why didn't we use that agent framework?"  →  m001
```

This is the hardest query in the set, and it is worth being explicit about why,
because it fails in all three modes and that is not a bug in the retriever.
m001 contains no token and no concept resembling "agent framework" — it names a
product (*Mastra*) and a failure mode (*suspend/resume durability*). Recovering
it from this query requires knowing that Mastra **is** an agent framework, which
is world knowledge held by the human and the calling LLM, not by a 47M-parameter
embedding model and not by BM25. The corpus additionally contains three memories
that are *legitimately* better matches for the literal words "agent" and
"framework" (m004 agent prompts, m012 no agent may write outside its worktree,
m009 the orchestrator plugin ABI), and the retriever ranks those first, which is
defensible behaviour.

It stays in the set at full weight. It was mandated by the sprint contract, it is
an honest example of the ceiling of embedding-only retrieval, and deleting the
one query that exposes a limitation is exactly the massaging this document
exists to prevent. Its cost is roughly 2.6 percentage points of recall@5.

### Adversarial construction

The eight adversarial queries were written by first covering the gold memory and
writing the question a colleague would actually ask, then checking the overlap
count and rewording any accidental token collisions. Examples:

| Query | Gold | Shared content words |
|---|---|---|
| q24 "What do we do when the same instruction arrives twice because the wire hiccuped?" | m030, m031 (idempotency) | none |
| q25 "Can a person who cannot use a mouse get all the way through?" | m054, m057 (keyboard a11y) | none |
| q27 "What keeps the books straight when a charge has to be undone?" | m032 (refunds as ledger entries) | none |
| q28 "Where should the fix-it steps sit relative to the thing that wakes you up?" | m051 (runbooks beside alerts) | none |
| q34 "What happens to an entry that turns out to be much too long?" | m043 (8 KB log truncation) | none |

They remain ordinary English. No query is a single-word trick, none uses a rare
term planted in the memory, and none was reworded after seeing which document
the binary returned — see the anti-overfitting rules below.

---

## 4. Authorship process and the holdout — stated honestly

The sprint contract permits test-driving the binary while authoring, and warns
against overfitting. Here is exactly what happened, including the parts that
weaken the result.

**Phase 1 — corpus (no test-driving).** All 62 memories were written before the
binary was run even once against them. Nothing in `memories.jsonl` was chosen,
reworded, or removed in response to a retrieval result. The corpus is frozen and
was never revised after Phase 2 began.

**Phase 2 — first 30 queries (test-driving permitted, and used).** Queries
q01–q30 were authored, then the harness was run once against them. The observed
outcome (semantic ≫ hybrid ≫ lexical) was recorded. **No query was edited, and
no gold label was changed, in response to that run.** The run's only influence
on the artifacts was diagnostic: it is what prompted the fusion analysis in
`REPORT.md`. Test-driving was also used during authoring in the narrow,
mechanical sense described above — checking content-word overlap counts and
rewording queries that accidentally reused a gold memory's vocabulary. That
tunes *away* from lexical leakage, not toward any particular ranking.

**Phase 3 — harness freeze.** `run_bench.py` was finalized and its hash recorded:

```
sha256(bench/run_bench.py) = 6b0d6e7139da660432ec3d05c793f43aa3620f337f5dc64c81ed145ae085218c
```

The metric implementations, the gate thresholds and the aggregation logic have
not changed since that hash was taken.

**Phase 4 — holdout (blind).** Queries q31–q38 (8 queries, 21% of the set) were
written *after* the freeze and were **never run individually**. They went
straight into the full evaluation. They received no overlap check, no rewording,
and no gold-label revision. They are flagged `"holdout": true` in the JSONL and
the harness reports them as a separate aggregate.

**Why this matters.** The holdout is the only part of the set that is provably
free of author feedback, so it is the honest estimate. It reproduces the main
finding cleanly:

| Subset | mode | recall@5 | MRR |
|---|---|---|---|
| Full 38 | semantic | 0.811 | 0.670 |
| Full 38 | hybrid | 0.636 | 0.497 |
| Full 38 | lexical | 0.412 | 0.186 |
| Holdout 8 | semantic | 0.625 | 0.542 |
| Holdout 8 | hybrid | 0.500 | 0.400 |
| Holdout 8 | lexical | 0.375 | 0.115 |

Same ordering, same gap direction. The hybrid-below-semantic result is therefore
not an artifact of the author having seen the tuned-set numbers.

**Caveat worth stating plainly.** 8 queries is a small holdout; its absolute
numbers carry wide error bars and are lower across the board than the tuned set,
which is the expected direction. The *ordering* is the reliable signal, not the
levels. A future sprint that wants tighter confidence should extend the holdout,
not re-tune the first 30.

---

## 5. Anti-overfitting rules that were followed

1. No memory was edited after any benchmark run.
2. No query was edited after seeing which document it retrieved.
3. No gold label was changed to match a retrieval result.
4. No query is a single-word trick or relies on a rare token planted in a memory;
   every query reads as something an AI harness would genuinely ask on behalf of
   a user.
5. Queries whose gold answer is genuinely unrecoverable (q01) were kept, not
   pruned, and the failure is analysed rather than hidden.
6. The gate thresholds come from `architecture.md` §21/§25 and were fixed before
   any measurement; they were not adjusted to the observed numbers.

## 6. Known limitations

- **Single annotator.** Gold labels reflect one author's judgment. Several
  queries have defensible secondary answers that are scored as misses — q26
  ("how far back can I look") could arguably include m027, and q30's three golds
  could arguably include m002. This depresses all modes roughly equally, so
  ablation comparisons are unaffected, but absolute recall is a slight
  underestimate.
- **English only, one domain register.** All memories are software-engineering
  prose. Retrieval quality on other registers is unmeasured.
- **62 memories is a small corpus.** The lexical leg's `LIMIT 200` never binds at
  this size, which materially changes RRF fusion behaviour; `REPORT.md` §4
  re-runs the whole set at 1K and 10K to separate that effect from the dataset.
- **No adversarial *content*.** Prompt-injection text and FTS/JSON metacharacters
  inside stored memories are a Sprint S6 security-review concern (§20), not a
  retrieval-quality concern, and are deliberately absent here.
