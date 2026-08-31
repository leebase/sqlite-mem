#!/usr/bin/env python3
"""
sqlite-mem operational metrics + retrieval-at-scale control (Sprint S5).

DEV TOOLING ONLY -- stdlib Python 3, no third-party imports, no network. The
sqlite-mem product is a single Rust binary and ships no Python; this script
drives the release binary as a black box, exactly as a real caller would.

Measures, at each corpus scale:
  * warm `ask` latency, median and p95 over >= 20 asks (both the binary's own
    reported elapsed_ms and the full process wall time)
  * `save` latency, median and p95
  * database file size (main + WAL/SHM if present)
  * cold start: process spawn -> JSON on stdout, median of 5 (this is the same
    thing as warm ask wall time for a CLI that exits per invocation, but is
    measured separately on a freshly-opened DB with the OS page cache cold-ish)

Scales are produced by deterministic synthetic inflation: filler memories are
generated from a fixed seed so any rerun produces byte-identical filler.

It also re-runs the golden query set at each inflated scale (the "scale control"),
because RRF fusion behaviour depends on how much of the corpus the lexical leg
manages to rank -- a property that changes with corpus size.

Usage:
  python3 bench/ops_bench.py --bin PATH --scratch DIR --out DIR
                             [--scales 62,1000,10000] [--skip-quality]
"""

import argparse
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from run_bench import (  # noqa: E402  (harness reuse; run_bench is frozen)
    Runner, load_jsonl, evaluate, aggregate, gate_verdicts, MODES,
)

CORPUS = os.path.join(HERE, "corpus")

# Deterministic filler vocabulary. Filler is written to look like plausible
# engineering memory prose so it competes for retrieval the way real corpus
# growth would -- pure lorem ipsum would be an unfairly easy distractor.
SUBJECTS = ["the ingest worker", "the batch scheduler", "the config loader",
            "the retry supervisor", "the export job", "the cache warmer",
            "the audit trail", "the shard rebalancer", "the token bucket",
            "the migration runner", "the health probe", "the queue drainer"]
VERBS = ["was tuned", "was rewritten", "was deprecated", "was instrumented",
         "was throttled", "was isolated", "was benchmarked", "was simplified"]
REASONS = ["after the p99 regressed under load",
           "because the previous approach leaked file descriptors",
           "so that a restart no longer loses in-flight work",
           "once the operator runbook proved too long to follow",
           "after a review flagged the coupling to the storage layer",
           "because the metric it emitted was never actionable",
           "to keep the change surface small for the next release",
           "since the original assumption about ordering no longer holds"]
TAILS = ["The change was reviewed and accepted without further comment.",
         "No follow-up work was scheduled at the time.",
         "The owner recorded the tradeoff in the sprint notes.",
         "A regression test was added alongside the change.",
         "The decision was revisited once and left unchanged."]
FILLER_PROJECTS = ["orbit", "cinder", "willow", "basalt", "meridian"]
FILLER_KINDS = ["decision", "constraint", "preference", "precedent", "review"]


def make_filler(n, seed=20260831):
    """Deterministic synthetic memories. Same seed -> byte-identical output."""
    rng = random.Random(seed)
    out = []
    for i in range(n):
        sent = "%s %s %s." % (rng.choice(SUBJECTS).capitalize(),
                              rng.choice(VERBS), rng.choice(REASONS))
        body = sent + " " + rng.choice(TAILS)
        out.append({
            "id": "f%06d" % i,
            "content": "Filler note %d. %s" % (i, body),
            "meta": {
                "project": rng.choice(FILLER_PROJECTS),
                "kind": rng.choice(FILLER_KINDS),
                "status": "current",
                "source": "notes/filler-%04d.md" % (i % 500),
            },
        })
    return out


def pct(values, p):
    if not values:
        return 0.0
    s = sorted(values)
    idx = min(len(s) - 1, max(0, int(round(p * (len(s) - 1)))))
    return s[idx]


def db_bytes(db_path):
    total = 0
    for suffix in ("", "-wal", "-shm"):
        p = db_path + suffix
        if os.path.exists(p):
            total += os.path.getsize(p)
    return total


def measure_asks(runner, queries, repeats):
    """Warm-path ask timing. Returns (wall_ms list, elapsed_ms list)."""
    wall, inner = [], []
    i = 0
    while len(wall) < repeats:
        q = queries[i % len(queries)]
        resp = runner.ask(q["query"], mode="hybrid", k=5)
        wall.append(resp["_wall_ms"])
        inner.append(resp.get("stats", {}).get("elapsed_ms", 0))
        i += 1
    return wall, inner


def peak_rss_kb(binary, db_path, query, env):
    """Peak RSS of one ask via /usr/bin/time -v, or None if unavailable."""
    if not os.path.exists("/usr/bin/time"):
        return None
    proc = subprocess.run(
        ["/usr/bin/time", "-v", binary, "ask", "--db", db_path,
         "--mode", "hybrid", "--k", "5", "--query", query],
        capture_output=True, text=True, env=env)
    for line in proc.stderr.splitlines():
        if "Maximum resident set size" in line:
            try:
                return int(line.rsplit(":", 1)[1].strip())
            except ValueError:
                return None
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--scales", default="62,1000,10000",
                    help="comma-separated total memory counts")
    ap.add_argument("--repeats", type=int, default=20)
    ap.add_argument("--skip-quality", action="store_true",
                    help="skip the retrieval-quality re-run at inflated scales")
    args = ap.parse_args()

    os.makedirs(args.scratch, exist_ok=True)
    os.makedirs(args.out, exist_ok=True)

    memories = load_jsonl(os.path.join(CORPUS, "memories.jsonl"))
    queries = sorted(load_jsonl(os.path.join(CORPUS, "queries.jsonl")),
                     key=lambda q: q["id"])
    scales = [int(s) for s in args.scales.split(",")]
    base = len(memories)

    report = {
        "binary": os.path.abspath(args.bin),
        "binary_size_bytes": os.path.getsize(args.bin),
        "base_corpus": base,
        "scales": [],
    }

    db_path = os.path.join(args.scratch, "ops.db")
    for suffix in ("", "-wal", "-shm", ".bak"):
        if os.path.exists(db_path + suffix):
            os.remove(db_path + suffix)

    runner = Runner(args.bin, db_path)
    env = runner.env

    # The golden corpus goes in first and stays; filler is appended in stages so
    # each scale is a strict superset of the previous one (single DB, grown).
    print("loading %d golden memories" % base, file=sys.stderr)
    save_lat = []
    id_map = {}
    for mem in memories:
        meta = dict(mem["meta"])
        src = meta.pop("source", None)
        t0 = time.perf_counter()
        resp = runner.save(mem["content"], meta, src)
        save_lat.append((time.perf_counter() - t0) * 1000.0)
        id_map[mem["id"]] = resp["id"]
    db_to_bench = {v: k for k, v in id_map.items()}

    filler_all = make_filler(max(scales))
    loaded = base

    for scale in scales:
        need = scale - loaded
        if need > 0:
            print("inflating to %d (adding %d filler)" % (scale, need), file=sys.stderr)
            for j in range(need):
                mem = filler_all[loaded - base + j]
                meta = dict(mem["meta"])
                src = meta.pop("source", None)
                t0 = time.perf_counter()
                runner.save(mem["content"], meta, src)
                save_lat.append((time.perf_counter() - t0) * 1000.0)
                if (j + 1) % 500 == 0:
                    print("  +%d/%d" % (j + 1, need), file=sys.stderr)
            loaded = scale

        print("measuring at scale %d" % scale, file=sys.stderr)
        wall, inner = measure_asks(runner, queries, args.repeats)
        # Cold start: median of 5 spawns, each the first ask after a fresh open.
        cold = []
        for _ in range(5):
            r = runner.ask(queries[0]["query"], mode="hybrid", k=5)
            cold.append(r["_wall_ms"])
        rss = peak_rss_kb(args.bin, db_path, queries[0]["query"], env)

        entry = {
            "memories": scale,
            "db_bytes": db_bytes(db_path),
            "ask_wall_ms": {"n": len(wall),
                            "median": round(statistics.median(wall), 2),
                            "p95": round(pct(wall, 0.95), 2),
                            "min": round(min(wall), 2), "max": round(max(wall), 2)},
            "ask_engine_ms": {"median": round(statistics.median(inner), 2),
                              "p95": round(pct(inner, 0.95), 2)},
            "cold_start_ms": {"n": len(cold),
                              "median": round(statistics.median(cold), 2)},
            "save_ms_cumulative": {"n": len(save_lat),
                                   "median": round(statistics.median(save_lat), 2),
                                   "p95": round(pct(save_lat, 0.95), 2)},
            "peak_rss_kb": rss,
        }

        if not args.skip_quality:
            rows = evaluate(runner, queries, db_to_bench, k=5)
            agg = aggregate(rows, "unfiltered")
            entry["quality_unfiltered"] = agg["overall"]
            entry["gates"] = gate_verdicts(agg)

        report["scales"].append(entry)
        with open(os.path.join(args.out, "ops.json"), "w", encoding="utf-8") as fh:
            json.dump(report, fh, indent=2, sort_keys=True)
            fh.write("\n")

    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
