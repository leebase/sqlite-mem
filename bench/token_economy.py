#!/usr/bin/env python3
"""
Token-economy estimate for sqlite-mem (Sprint S5, reported not gated).

DEV TOOLING ONLY -- stdlib Python 3, no third-party imports, no network.

The claim under test (architecture.md §3, §21.4): an AI that asks sqlite-mem
reads a few distilled memories instead of exploring a file tree, and that
difference is measurable in tokens.

Two baselines are computed for each query, because they bracket what a real
harness actually does:

  A. FULL-CORPUS SCAN -- the harness has no index and reads every memory-bearing
     document to find the answer. This is the upper bound: it is what "grep the
     whole knowledge base into context" costs.

  B. ASK --k 5 -- the tokens actually returned on stdout by one ask, i.e. what
     the harness puts into its context window.

Tokens are estimated as ceil(chars / 4), the standard rough English heuristic.
This is an estimate, not a tokenizer count, and is labelled as such everywhere.

Both baselines are measured on the same corpus, so the ratio is the meaningful
number; the absolute token counts scale with corpus size.

Usage:
  python3 bench/token_economy.py --bin PATH --db PATH [--n 10]
"""

import argparse
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from run_bench import Runner, load_jsonl  # noqa: E402

CORPUS = os.path.join(HERE, "corpus")


def toks(text):
    return math.ceil(len(text) / 4)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--db", required=True, help="a DB already loaded with the golden corpus")
    ap.add_argument("--n", type=int, default=10, help="number of representative queries")
    ap.add_argument("--out")
    args = ap.parse_args()

    memories = load_jsonl(os.path.join(CORPUS, "memories.jsonl"))
    queries = sorted(load_jsonl(os.path.join(CORPUS, "queries.jsonl")),
                     key=lambda q: q["id"])

    # Representative sample: every 4th query across the id-sorted set, so all
    # four categories and both holdout/non-holdout are represented. Deterministic.
    step = max(1, len(queries) // args.n)
    sample = queries[::step][:args.n]

    # Baseline A: the whole corpus as a harness would have to read it. Each
    # memory is rendered the way it would appear in a Markdown source file --
    # the statement plus the provenance line a reader needs to judge it.
    corpus_text = "\n\n".join(
        "%s\n(project: %s, kind: %s, source: %s)"
        % (m["content"], m["meta"]["project"], m["meta"]["kind"], m["meta"]["source"])
        for m in memories)
    corpus_tokens = toks(corpus_text)

    runner = Runner(args.bin, args.db)
    rows = []
    for q in sample:
        resp = runner.ask(q["query"], mode="hybrid", k=5)
        raw = json.dumps({k: v for k, v in resp.items() if k != "_wall_ms"},
                         separators=(",", ":"))
        ask_tokens = toks(raw)
        content_only = "\n\n".join(r["content"] for r in resp.get("results", []))
        rows.append({
            "query_id": q["id"],
            "query": q["query"],
            "ask_tokens_full_json": ask_tokens,
            "ask_tokens_content_only": toks(content_only),
            "returned": len(resp.get("results", [])),
        })

    med_json = sorted(r["ask_tokens_full_json"] for r in rows)[len(rows) // 2]
    med_content = sorted(r["ask_tokens_content_only"] for r in rows)[len(rows) // 2]
    out = {
        "method": "tokens estimated as ceil(chars/4)",
        "corpus_memories": len(memories),
        "baseline_full_corpus_scan_tokens": corpus_tokens,
        "queries_sampled": len(rows),
        "ask_k5_tokens_full_json_median": med_json,
        "ask_k5_tokens_content_only_median": med_content,
        "reduction_vs_full_scan_full_json": round(corpus_tokens / med_json, 1),
        "reduction_vs_full_scan_content_only": round(corpus_tokens / med_content, 1),
        "per_query": rows,
    }
    text = json.dumps(out, indent=2, sort_keys=True)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
