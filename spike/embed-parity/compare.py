#!/usr/bin/env python3
"""S1 parity comparison: Candle vectors vs sentence-transformers reference.

Usage: python3 compare.py <candle.json> <reference.json>
Gate (architecture.md §26 / project-plan.md S1): cosine >= 0.999 for every text.
Stdlib only; exit 0 pass / 1 fail / 2 structural error.
"""
import json
import math
import sys


def cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    return dot / (na * nb) if na and nb else 0.0


def main(candle_path, ref_path):
    candle = json.load(open(candle_path))
    ref = json.load(open(ref_path))

    cv, rv = candle["vectors"], ref["vectors"]
    if set(cv) != set(rv):
        print(f"ERROR: id mismatch: candle-only={sorted(set(cv)-set(rv))[:5]} "
              f"ref-only={sorted(set(rv)-set(cv))[:5]}")
        return 2
    dims = {len(v) for v in cv.values()} | {len(v) for v in rv.values()}
    if dims != {384}:
        print(f"ERROR: unexpected dims {dims}")
        return 2

    sims = sorted(((cosine(cv[k], rv[k]), k) for k in cv))
    vals = [s for s, _ in sims]
    n = len(vals)
    mean = sum(vals) / n
    print(f"candle={candle.get('model')} ref={ref.get('model')} texts={n}")
    print(f"cosine: min={vals[0]:.6f} p05={vals[n//20]:.6f} "
          f"median={vals[n//2]:.6f} mean={mean:.6f} max={vals[-1]:.6f}")
    print("worst 5:")
    for s, k in sims[:5]:
        print(f"  {k}: {s:.6f}")
    failures = [(s, k) for s, k in sims if s < 0.999]
    if failures:
        print(f"FAIL: {len(failures)}/{n} texts below 0.999 gate")
        return 1
    print(f"PASS: all {n} texts >= 0.999")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1], sys.argv[2]))
