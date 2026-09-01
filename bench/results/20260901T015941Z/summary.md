## Unfiltered (all evaluated queries) -- overall

| mode     | n  | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|----|----------|----------|--------|--------|
| hybrid   | 38 | 0.5219   | 0.8114   | 0.6697 | 0.6864 |
| lexical  | 38 | 0.0790   | 0.4123   | 0.2026 | 0.2510 |
| semantic | 38 | 0.5219   | 0.8114   | 0.6697 | 0.6864 |

## Unfiltered (all evaluated queries) -- by category

| category        | mode     | n  | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|----|----------|----------|--------|--------|
| adversarial     | hybrid   | 8  | 0.3750   | 0.6875   | 0.5833 | 0.5766 |
| adversarial     | lexical  | 8  | 0.0000   | 0.1875   | 0.0938 | 0.1119 |
| adversarial     | semantic | 8  | 0.3750   | 0.6875   | 0.5833 | 0.5766 |
| cross-wording   | hybrid   | 15 | 0.5889   | 0.8889   | 0.7667 | 0.7680 |
| cross-wording   | lexical  | 15 | 0.0667   | 0.3444   | 0.1856 | 0.2192 |
| cross-wording   | semantic | 15 | 0.5889   | 0.8889   | 0.7667 | 0.7680 |
| metadata-scoped | hybrid   | 8  | 0.5000   | 0.6250   | 0.5625 | 0.5789 |
| metadata-scoped | lexical  | 8  | 0.1250   | 0.6250   | 0.2812 | 0.3654 |
| metadata-scoped | semantic | 8  | 0.5000   | 0.6250   | 0.5625 | 0.5789 |
| precedent       | hybrid   | 7  | 0.5714   | 1.0000   | 0.6833 | 0.7597 |
| precedent       | lexical  | 7  | 0.1429   | 0.5714   | 0.2738 | 0.3472 |
| precedent       | semantic | 7  | 0.5714   | 1.0000   | 0.6833 | 0.7597 |

## Filtered (metadata-scoped queries only) -- overall

| mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|---|----------|----------|--------|--------|
| hybrid   | 8 | 0.7500   | 0.7500   | 0.7500 | 0.7500 |
| lexical  | 8 | 0.6250   | 0.6250   | 0.6250 | 0.6250 |
| semantic | 8 | 0.7500   | 0.7500   | 0.7500 | 0.7500 |

## Filtered (metadata-scoped queries only) -- by category

| category        | mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|---|----------|----------|--------|--------|
| metadata-scoped | hybrid   | 8 | 0.7500   | 0.7500   | 0.7500 | 0.7500 |
| metadata-scoped | lexical  | 8 | 0.6250   | 0.6250   | 0.6250 | 0.6250 |
| metadata-scoped | semantic | 8 | 0.7500   | 0.7500   | 0.7500 | 0.7500 |

## Holdout subset -- overall

| mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|----------|---|----------|----------|--------|--------|
| hybrid   | 8 | 0.5000   | 0.6250   | 0.5417 | 0.5625 |
| lexical  | 8 | 0.0000   | 0.3750   | 0.1146 | 0.1788 |
| semantic | 8 | 0.5000   | 0.6250   | 0.5417 | 0.5625 |

## Holdout subset -- by category

| category        | mode     | n | recall@1 | recall@5 | MRR    | nDCG@5 |
|-----------------|----------|---|----------|----------|--------|--------|
| adversarial     | hybrid   | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| adversarial     | lexical  | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| adversarial     | semantic | 2 | 0.5000   | 0.5000   | 0.5000 | 0.5000 |
| cross-wording   | hybrid   | 3 | 0.6667   | 1.0000   | 0.7778 | 0.8333 |
| cross-wording   | lexical  | 3 | 0.0000   | 0.3333   | 0.1111 | 0.1667 |
| cross-wording   | semantic | 3 | 0.6667   | 1.0000   | 0.7778 | 0.8333 |
| metadata-scoped | hybrid   | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| metadata-scoped | lexical  | 2 | 0.0000   | 0.5000   | 0.1250 | 0.2153 |
| metadata-scoped | semantic | 2 | 0.0000   | 0.0000   | 0.0000 | 0.0000 |
| precedent       | hybrid   | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |
| precedent       | lexical  | 1 | 0.0000   | 1.0000   | 0.3333 | 0.5000 |
| precedent       | semantic | 1 | 1.0000   | 1.0000   | 1.0000 | 1.0000 |

## Gate verdicts (unfiltered, all evaluated queries)


| gate                        | value  | threshold | verdict |
|-----------------------------|--------|-----------|---------|
| hybrid recall@5 >= 0.85     | 0.8114 | 0.8500    | FAIL    |
| hybrid MRR >= 0.70          | 0.6697 | 0.7000    | FAIL    |
| hybrid recall@5 >= lexical  | 0.8114 | 0.4123    | PASS    |
| hybrid recall@5 >= semantic | 0.8114 | 0.8114    | PASS    |
| hybrid MRR >= lexical MRR   | 0.6697 | 0.2026    | PASS    |
| hybrid MRR >= semantic MRR  | 0.6697 | 0.6697    | PASS    |
