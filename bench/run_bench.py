#!/usr/bin/env python3
"""
sqlite-mem retrieval benchmark harness (Sprint S5).

DEV TOOLING ONLY. The sqlite-mem product is a single self-contained Rust binary
and ships no Python whatsoever: nothing in this file is compiled, packaged, or
distributed with the product. Python is used here purely because the benchmark
is a measurement script that drives the release binary as a black box through
its documented CLI/JSON contract, which is exactly the interface a real caller
(Folder Chief, or any other AI harness) sees.

Stdlib only, by rule -- no third-party imports, no network access. Python 3.8+.

What it does:
  1. Builds a fresh database in a scratch directory.
  2. Saves every memory from corpus/memories.jsonl via `sqlite-mem save`,
     passing each metadata pair as a --meta flag and `source` as --source.
  3. Runs every query in corpus/queries.jsonl in each retrieval mode
     (hybrid / lexical / semantic). Queries carrying a `filter` are additionally
     run with those --where terms applied (the "filtered" ablation cell); they
     are also run unfiltered so the filtered-vs-unfiltered contrast is visible.
  4. Parses the JSON, computes recall@1, recall@5, MRR and nDCG@5 per query,
     aggregated per category and overall.
  5. Writes results/<timestamp>/results.json plus a deterministic summary table.

Usage:
  python3 bench/run_bench.py --selftest
  python3 bench/run_bench.py --bin /path/to/sqlite-mem [--out DIR] [--scratch DIR]
                             [--holdout-only | --no-holdout]

Environment:
  SQLITE_MEM_MODEL_DIR must point at the granite model directory when the binary
  was built with the default `model-sidecar` feature.
"""

import argparse
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus")
MODES = ("hybrid", "lexical", "semantic")


# --------------------------------------------------------------------------
# Metric math. Kept pure and side-effect free so --selftest can pin exact
# values against hand-computed fixtures.
# --------------------------------------------------------------------------

def recall_at_k(ranked, relevant, k):
    """Fraction of the gold set retrieved within the top k.

    ranked: list of memory ids, best first, no duplicates.
    relevant: set/list of gold memory ids (non-empty).
    """
    rel = set(relevant)
    if not rel:
        raise ValueError("recall_at_k requires a non-empty gold set")
    hits = sum(1 for mid in ranked[:k] if mid in rel)
    return hits / len(rel)


def reciprocal_rank(ranked, relevant):
    """1 / (1-based rank of the first gold hit); 0.0 if none present."""
    rel = set(relevant)
    for i, mid in enumerate(ranked):
        if mid in rel:
            return 1.0 / (i + 1)
    return 0.0


def dcg(gains):
    """Discounted cumulative gain with the standard log2(rank+1) discount."""
    return sum(g / math.log2(i + 2) for i, g in enumerate(gains))


def ndcg_at_k(ranked, relevant, k):
    """Binary-gain nDCG@k. Ideal ranking places min(|rel|, k) gold docs first."""
    rel = set(relevant)
    if not rel:
        raise ValueError("ndcg_at_k requires a non-empty gold set")
    gains = [1.0 if mid in rel else 0.0 for mid in ranked[:k]]
    ideal = [1.0] * min(len(rel), k)
    idcg = dcg(ideal)
    if idcg == 0.0:
        return 0.0
    return dcg(gains) / idcg


def mean(xs):
    return statistics.fmean(xs) if xs else 0.0


# --------------------------------------------------------------------------
# Self-test: hand-computed fixtures, exact assertions.
# --------------------------------------------------------------------------

def selftest():
    def close(a, b, label):
        assert abs(a - b) < 1e-12, "%s: got %.17g expected %.17g" % (label, a, b)

    # Fixture A -- single gold doc at rank 1.
    ranked = ["a", "b", "c", "d", "e"]
    gold = ["a"]
    close(recall_at_k(ranked, gold, 1), 1.0, "A recall@1")
    close(recall_at_k(ranked, gold, 5), 1.0, "A recall@5")
    close(reciprocal_rank(ranked, gold), 1.0, "A MRR")
    close(ndcg_at_k(ranked, gold, 5), 1.0, "A nDCG@5")

    # Fixture B -- single gold doc at rank 3.
    # recall@1 = 0/1 = 0; recall@5 = 1/1 = 1; RR = 1/3.
    # DCG = 1/log2(4) = 0.5 ; IDCG = 1/log2(2) = 1 ; nDCG = 0.5.
    gold = ["c"]
    close(recall_at_k(ranked, gold, 1), 0.0, "B recall@1")
    close(recall_at_k(ranked, gold, 5), 1.0, "B recall@5")
    close(reciprocal_rank(ranked, gold), 1.0 / 3.0, "B MRR")
    close(ndcg_at_k(ranked, gold, 5), 0.5, "B nDCG@5")

    # Fixture C -- two gold docs, at ranks 2 and 4.
    # recall@1 = 0/2 = 0 ; recall@5 = 2/2 = 1 ; RR = 1/2.
    # DCG = 1/log2(3) + 1/log2(5) = 0.6309297535714574 + 0.43067655807339306
    #     = 1.0616063116448505
    # IDCG = 1/log2(2) + 1/log2(3) = 1 + 0.6309297535714574 = 1.6309297535714574
    # nDCG = 0.6509209298071326
    gold = ["b", "d"]
    close(recall_at_k(ranked, gold, 1), 0.0, "C recall@1")
    close(recall_at_k(ranked, gold, 5), 1.0, "C recall@5")
    close(reciprocal_rank(ranked, gold), 0.5, "C MRR")
    close(ndcg_at_k(ranked, gold, 5), 1.0616063116448505 / 1.6309297535714574, "C nDCG@5")
    close(ndcg_at_k(ranked, gold, 5), 0.6509209298071326, "C nDCG@5 literal")

    # Fixture D -- gold doc absent from the ranking entirely.
    gold = ["zzz"]
    close(recall_at_k(ranked, gold, 5), 0.0, "D recall@5")
    close(reciprocal_rank(ranked, gold), 0.0, "D MRR")
    close(ndcg_at_k(ranked, gold, 5), 0.0, "D nDCG@5")

    # Fixture E -- three gold docs but only k=5 slots, two retrieved at 1 and 5.
    # recall@5 = 2/3 ; RR = 1 ; DCG = 1/log2(2) + 1/log2(6) = 1 + 0.3868528072345416
    #                                = 1.3868528072345416
    # IDCG (min(3,5)=3 ideal hits) = 1 + 1/log2(3) + 1/log2(4)
    #                              = 1 + 0.6309297535714574 + 0.5 = 2.1309297535714574
    # nDCG = 0.6508191671189614
    ranked_e = ["a", "x", "y", "z", "c"]
    gold_e = ["a", "c", "q"]
    close(recall_at_k(ranked_e, gold_e, 1), 1.0 / 3.0, "E recall@1")
    close(recall_at_k(ranked_e, gold_e, 5), 2.0 / 3.0, "E recall@5")
    close(reciprocal_rank(ranked_e, gold_e), 1.0, "E MRR")
    close(ndcg_at_k(ranked_e, gold_e, 5), 1.3868528072345416 / 2.1309297535714574, "E nDCG@5")

    # Fixture F -- empty result list.
    close(recall_at_k([], ["a"], 5), 0.0, "F recall@5")
    close(reciprocal_rank([], ["a"]), 0.0, "F MRR")
    close(ndcg_at_k([], ["a"], 5), 0.0, "F nDCG@5")

    # Fixture G -- dcg building block, checked directly.
    close(dcg([1.0]), 1.0, "G dcg[1]")
    close(dcg([0.0, 1.0]), 1.0 / math.log2(3), "G dcg[0,1]")

    # Fixture H -- aggregate mean.
    close(mean([1.0, 0.0, 0.5]), 0.5, "H mean")

    # Guard rails.
    for fn in (recall_at_k, ndcg_at_k):
        try:
            fn(ranked, [], 5)
        except ValueError:
            pass
        else:
            raise AssertionError("%s must reject an empty gold set" % fn.__name__)

    print("selftest: OK (8 fixture groups, exact values asserted)")
    return 0


# --------------------------------------------------------------------------
# Corpus IO
# --------------------------------------------------------------------------

def load_jsonl(path):
    rows = []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise SystemExit("%s:%d: bad JSON: %s" % (path, lineno, exc))
    return rows


# --------------------------------------------------------------------------
# Binary driver
# --------------------------------------------------------------------------

class Runner:
    def __init__(self, binary, db_path, env=None):
        self.binary = binary
        self.db_path = db_path
        self.env = dict(os.environ)
        if env:
            self.env.update(env)

    def _run(self, args, stdin_text=None):
        proc = subprocess.run(
            [self.binary] + args,
            input=stdin_text,
            capture_output=True,
            text=True,
            env=self.env,
        )
        return proc

    def save(self, content, meta, source=None):
        args = ["save", "--db", self.db_path, "--stdin"]
        for key in sorted(meta):
            args += ["--meta", "%s=%s" % (key, meta[key])]
        if source:
            args += ["--source", source]
        proc = self._run(args, stdin_text=content)
        if proc.returncode != 0:
            raise SystemExit(
                "save failed (exit %d)\nargs: %r\nstdout: %s\nstderr: %s"
                % (proc.returncode, args, proc.stdout, proc.stderr)
            )
        return json.loads(proc.stdout)

    def ask(self, query, mode="hybrid", k=5, where=None):
        args = ["ask", "--db", self.db_path, "--mode", mode, "--k", str(k), "--stdin"]
        for term in (where or []):
            args += ["--where", term]
        t0 = time.perf_counter()
        proc = self._run(args, stdin_text=query)
        wall_ms = (time.perf_counter() - t0) * 1000.0
        if proc.returncode != 0:
            raise SystemExit(
                "ask failed (exit %d)\nargs: %r\nstdout: %s\nstderr: %s"
                % (proc.returncode, args, proc.stdout, proc.stderr)
            )
        payload = json.loads(proc.stdout)
        payload["_wall_ms"] = wall_ms
        return payload


# --------------------------------------------------------------------------
# Corpus load into a fresh DB
# --------------------------------------------------------------------------

def build_db(runner, memories, verbose=True):
    """Save every memory; return {bench_id: db_id} and save latencies (ms)."""
    id_map = {}
    latencies = []
    for i, mem in enumerate(memories, 1):
        meta = dict(mem.get("meta", {}))
        source = meta.pop("source", None)
        t0 = time.perf_counter()
        resp = runner.save(mem["content"], meta, source)
        latencies.append((time.perf_counter() - t0) * 1000.0)
        if not resp.get("ok"):
            raise SystemExit("save returned ok=false for %s: %r" % (mem["id"], resp))
        id_map[mem["id"]] = resp["id"]
        if verbose and i % 10 == 0:
            print("  saved %d/%d" % (i, len(memories)), file=sys.stderr)
    return id_map, latencies


# --------------------------------------------------------------------------
# Evaluation
# --------------------------------------------------------------------------

def ranked_bench_ids(response, db_to_bench):
    """Map the binary's ranked result ids back to bench corpus ids."""
    out = []
    for row in response.get("results", []):
        bid = db_to_bench.get(row["id"])
        if bid is not None:
            out.append(bid)
    return out


def evaluate(runner, queries, db_to_bench, k=5):
    """Run every query in every mode (and filtered variant); return raw rows."""
    rows = []
    for q in queries:
        gold = q["relevant"]
        has_filter = bool(q.get("filter"))
        cells = [("unfiltered", None)]
        if has_filter:
            cells.append(("filtered", q["filter"]))
        for cell_name, where in cells:
            for mode in MODES:
                resp = runner.ask(q["query"], mode=mode, k=k, where=where)
                ranked = ranked_bench_ids(resp, db_to_bench)
                rows.append({
                    "query_id": q["id"],
                    "query": q["query"],
                    "category": q["category"],
                    "holdout": bool(q.get("holdout", False)),
                    "cell": cell_name,
                    "mode": mode,
                    "gold": list(gold),
                    "ranked": ranked,
                    "recall@1": recall_at_k(ranked, gold, 1),
                    "recall@5": recall_at_k(ranked, gold, 5),
                    "mrr": reciprocal_rank(ranked, gold),
                    "ndcg@5": ndcg_at_k(ranked, gold, k),
                    "candidates": resp.get("stats", {}).get("candidates"),
                    "elapsed_ms": resp.get("stats", {}).get("elapsed_ms"),
                    "wall_ms": round(resp["_wall_ms"], 3),
                })
    rows.sort(key=lambda r: (r["query_id"], r["cell"], r["mode"]))
    return rows


METRICS = ("recall@1", "recall@5", "mrr", "ndcg@5")


def aggregate(rows, cell="unfiltered", subset=None):
    """Aggregate metrics by mode, overall and per category.

    subset: optional predicate over a row, e.g. holdout-only.
    """
    out = {"overall": {}, "by_category": {}}
    sel = [r for r in rows if r["cell"] == cell and (subset is None or subset(r))]
    cats = sorted({r["category"] for r in sel})
    for mode in MODES:
        mrows = [r for r in sel if r["mode"] == mode]
        out["overall"][mode] = {
            "n": len(mrows),
            **{m: round(mean([r[m] for r in mrows]), 5) for m in METRICS},
        }
    for cat in cats:
        out["by_category"][cat] = {}
        for mode in MODES:
            mrows = [r for r in sel if r["mode"] == mode and r["category"] == cat]
            out["by_category"][cat][mode] = {
                "n": len(mrows),
                **{m: round(mean([r[m] for r in mrows]), 5) for m in METRICS},
            }
    return out


def gate_verdicts(agg):
    h = agg["overall"]["hybrid"]
    lex = agg["overall"]["lexical"]
    sem = agg["overall"]["semantic"]
    gates = []
    # v1 gates per D016.3 (architecture.md §21.1, recalibrated on S5/S5b
    # evidence). The original pre-evidence 0.85/0.70 rows are kept, labeled
    # historical, so old reports remain interpretable.
    gates.append(("hybrid recall@5 >= 0.80 (v1 gate, D016.3)", h["recall@5"], 0.80, h["recall@5"] >= 0.80))
    gates.append(("hybrid MRR >= 0.65 (v1 gate, D016.3)", h["mrr"], 0.65, h["mrr"] >= 0.65))
    gates.append(("[historical pre-evidence] recall@5 >= 0.85", h["recall@5"], 0.85, h["recall@5"] >= 0.85))
    gates.append(("[historical pre-evidence] MRR >= 0.70", h["mrr"], 0.70, h["mrr"] >= 0.70))
    gates.append(("hybrid recall@5 >= lexical", h["recall@5"], lex["recall@5"],
                  h["recall@5"] >= lex["recall@5"]))
    gates.append(("hybrid recall@5 >= semantic", h["recall@5"], sem["recall@5"],
                  h["recall@5"] >= sem["recall@5"]))
    gates.append(("hybrid MRR >= lexical MRR", h["mrr"], lex["mrr"], h["mrr"] >= lex["mrr"]))
    gates.append(("hybrid MRR >= semantic MRR", h["mrr"], sem["mrr"], h["mrr"] >= sem["mrr"]))
    return [{"gate": g, "value": v, "threshold": t, "pass": bool(p)} for g, v, t, p in gates]


# --------------------------------------------------------------------------
# Reporting
# --------------------------------------------------------------------------

def fmt_table(headers, rows):
    widths = [len(h) for h in headers]
    for r in rows:
        for i, c in enumerate(r):
            widths[i] = max(widths[i], len(str(c)))
    line = "| " + " | ".join(h.ljust(widths[i]) for i, h in enumerate(headers)) + " |"
    sep = "|-" + "-|-".join("-" * w for w in widths) + "-|"
    body = ["| " + " | ".join(str(c).ljust(widths[i]) for i, c in enumerate(r)) + " |"
            for r in rows]
    return "\n".join([line, sep] + body)


def summary_text(agg, label):
    parts = ["## %s -- overall" % label, ""]
    parts.append(fmt_table(
        ["mode", "n", "recall@1", "recall@5", "MRR", "nDCG@5"],
        [[m, agg["overall"][m]["n"]] + ["%.4f" % agg["overall"][m][k] for k in METRICS]
         for m in MODES]))
    parts += ["", "## %s -- by category" % label, ""]
    body = []
    for cat in sorted(agg["by_category"]):
        for m in MODES:
            c = agg["by_category"][cat][m]
            body.append([cat, m, c["n"]] + ["%.4f" % c[k] for k in METRICS])
    parts.append(fmt_table(
        ["category", "mode", "n", "recall@1", "recall@5", "MRR", "nDCG@5"], body))
    return "\n".join(parts)


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--selftest", action="store_true",
                    help="run metric-math fixtures and exit")
    ap.add_argument("--bin", help="path to the sqlite-mem release binary")
    ap.add_argument("--out", help="results directory (default bench/results/<timestamp>)")
    ap.add_argument("--scratch", help="scratch dir for the benchmark DB")
    ap.add_argument("--k", type=int, default=5)
    ap.add_argument("--holdout-only", action="store_true",
                    help="evaluate only queries flagged holdout=true")
    ap.add_argument("--no-holdout", action="store_true",
                    help="evaluate only queries flagged holdout=false")
    ap.add_argument("--keep-db", action="store_true", help="do not delete the scratch DB")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if not args.bin:
        ap.error("--bin is required unless --selftest is given")

    memories = load_jsonl(os.path.join(CORPUS, "memories.jsonl"))
    queries = load_jsonl(os.path.join(CORPUS, "queries.jsonl"))
    if args.holdout_only:
        queries = [q for q in queries if q.get("holdout")]
    elif args.no_holdout:
        queries = [q for q in queries if not q.get("holdout")]
    queries.sort(key=lambda q: q["id"])

    scratch = args.scratch or tempfile.mkdtemp(prefix="sqlite-mem-bench-")
    os.makedirs(scratch, exist_ok=True)
    db_path = os.path.join(scratch, "bench.db")
    for suffix in ("", "-wal", "-shm", ".bak"):
        p = db_path + suffix
        if os.path.exists(p):
            os.remove(p)

    runner = Runner(args.bin, db_path)
    print("loading %d memories into %s" % (len(memories), db_path), file=sys.stderr)
    id_map, save_lat = build_db(runner, memories)
    db_to_bench = {v: k for k, v in id_map.items()}
    if len(db_to_bench) != len(id_map):
        raise SystemExit("collision: two bench memories mapped to one db id "
                         "(unexpected dedup?)")

    print("evaluating %d queries x %d modes" % (len(queries), len(MODES)), file=sys.stderr)
    rows = evaluate(runner, queries, db_to_bench, k=args.k)

    agg_unfiltered = aggregate(rows, "unfiltered")
    filtered_rows = [r for r in rows if r["cell"] == "filtered"]
    agg_filtered = aggregate(rows, "filtered") if filtered_rows else None
    holdout_rows = [r for r in rows if r["holdout"]]
    agg_holdout = (aggregate(rows, "unfiltered", subset=lambda r: r["holdout"])
                   if holdout_rows else None)

    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    outdir = args.out or os.path.join(HERE, "results", ts)
    os.makedirs(outdir, exist_ok=True)

    payload = {
        "generated_at_utc": ts,
        "binary": os.path.abspath(args.bin),
        "k": args.k,
        "corpus": {"memories": len(memories), "queries": len(queries)},
        "save_latency_ms": {
            "n": len(save_lat),
            "median": round(statistics.median(save_lat), 3),
            "p95": round(sorted(save_lat)[max(0, int(0.95 * len(save_lat)) - 1)], 3),
            "mean": round(mean(save_lat), 3),
        },
        "aggregate_unfiltered": agg_unfiltered,
        "aggregate_filtered": agg_filtered,
        "aggregate_holdout_unfiltered": agg_holdout,
        "gates_unfiltered": gate_verdicts(agg_unfiltered),
        "gates_holdout": gate_verdicts(agg_holdout) if agg_holdout else None,
        "per_query": rows,
        "id_map": id_map,
    }
    results_path = os.path.join(outdir, "results.json")
    with open(results_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, indent=2, sort_keys=True)
        fh.write("\n")

    chunks = [summary_text(agg_unfiltered, "Unfiltered (all evaluated queries)")]
    if agg_filtered:
        chunks.append(summary_text(agg_filtered, "Filtered (metadata-scoped queries only)"))
    if agg_holdout:
        chunks.append(summary_text(agg_holdout, "Holdout subset"))
    chunks.append("## Gate verdicts (unfiltered, all evaluated queries)\n")
    chunks.append(fmt_table(
        ["gate", "value", "threshold", "verdict"],
        [[g["gate"], "%.4f" % g["value"], "%.4f" % g["threshold"],
          "PASS" if g["pass"] else "FAIL"] for g in payload["gates_unfiltered"]]))
    summary = "\n\n".join(chunks) + "\n"
    with open(os.path.join(outdir, "summary.md"), "w", encoding="utf-8") as fh:
        fh.write(summary)

    print(summary)
    print("wrote %s" % results_path, file=sys.stderr)
    if not args.keep_db and not args.scratch:
        shutil.rmtree(scratch, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
