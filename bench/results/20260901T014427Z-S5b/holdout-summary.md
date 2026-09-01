## Unfiltered (all evaluated queries) -- overall

| mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|---|----------|----------|--------|--------|
| hybrid   | 8 | 0.3750   | 0.5000   | 0.4000 | 0.4234 |
| lexical  | 8 | 0.0000   | 0.3750   | 0.1146 | 0.1788 |
| semantic | 8 | 0.5000   | 0.6250   | 0.5417 | 0.5625 |

## Unfiltered (all evaluated queries) -- by category

| category        | mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|---|----------|----------|--------|--------|
| adversarial     | hybrid   | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| adversarial     | lexical  | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| adversarial     | semantic | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| cross-wording   | hybrid   | 3 | 0.6667   | 0.6667   | 0.6667 | 0.6667 |
| cross-wording   | lexical  | 3 | 0.0000   | 0.3333   | 0.1111 | 0.1667 |
| cross-wording   | semantic | 3 | 0.6667   | 1.0000   | 0.7778 | 0.8333 |
| metadata-scoped | hybrid   | 2 | 0.0000   | 0.5000   | 0.1000 | 0.1934 |
| metadata-scoped | lexical  | 2 | 0.0000   | 0.5000   | 0.1250 | 0.2153 |
| metadata-scoped | semantic | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| precedent       | hybrid   | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |
| precedent       | lexical  | 1 | 0.0000   | 1.0000   | 0.3333 | 0.5000 |
| precedent       | semantic | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |

## Filtered (metadata-scoped queries only) -- overall

| mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|---|----------|----------|--------|--------|
| hybrid   | 2 | 0.0000   | 0.5000   | 0.1250 | 0.2153 |
| lexical  | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| semantic | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |

## Filtered (metadata-scoped queries only) -- by category

| category        | mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|---|----------|----------|--------|--------|
| metadata-scoped | hybrid   | 2 | 0.0000   | 0.5000   | 0.1250 | 0.2153 |
| metadata-scoped | lexical  | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| metadata-scoped | semantic | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |

## Holdout subset -- overall

| mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|---|----------|----------|--------|--------|
| hybrid   | 8 | 0.3750   | 0.5000   | 0.4000 | 0.4234 |
| lexical  | 8 | 0.0000   | 0.3750   | 0.1146 | 0.1788 |
| semantic | 8 | 0.5000   | 0.6250   | 0.5417 | 0.5625 |

## Holdout subset -- by category

| category        | mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|---|----------|----------|--------|--------|
| adversarial     | hybrid   | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| adversarial     | lexical  | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| adversarial     | semantic | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| cross-wording   | hybrid   | 3 | 0.6667   | 0.6667   | 0.6667 | 0.6667 |
| cross-wording   | lexical  | 3 | 0.0000   | 0.3333   | 0.1111 | 0.1667 |
| cross-wording   | semantic | 3 | 0.6667   | 1.0000   | 0.7778 | 0.8333 |
| metadata-scoped | hybrid   | 2 | 0.0000   | 0.5000   | 0.1000 | 0.1934 |
| metadata-scoped | lexical  | 2 | 0.0000   | 0.5000   | 0.1250 | 0.2153 |
| metadata-scoped | semantic | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| precedent       | hybrid   | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |
| precedent       | lexical  | 1 | 0.0000   | 1.0000   | 0.3333 | 0.5000 |
| precedent       | semantic | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |

## Gate verdicts (unfiltered, all evaluated queries)


| gate                        | value  | threshold | verdict |
|-----------------------------|--------|-----------|---------|
| hybrid recall@5 >= 0.85     | 0.5000 | 0.8500    | FAIL    |
| hybrid MRR >= 0.70          | 0.4000 | 0.7000    | FAIL    |
| hybrid recall@5 >= lexical  | 0.5000 | 0.3750    | PASS    |
| hybrid recall@5 >= semantic | 0.5000 | 0.6250    | FAIL    |
| hybrid MRR >= lexical MRR   | 0.4000 | 0.1146    | PASS    |
| hybrid MRR >= semantic MRR  | 0.4000 | 0.5417    | FAIL    |
