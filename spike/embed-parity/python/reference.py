#!/usr/bin/env python3
"""
Sprint S1 embedding parity spike -- Python/sentence-transformers reference vectors.

Generates ground-truth embedding vectors for the fixed corpus using the
official sentence-transformers implementation, with NO query/document
prefixes, NO prompt_name, and the model's own default max_seq_length
truncation left untouched. These vectors are the reference that the
Rust/Candle implementation (produced separately) is checked against.

Usage:
    /path/to/pyref-venv/bin/python reference.py

Writes:
    ../out/reference_granite.json
    ../out/reference_bge.json
"""

import json
import pathlib
import sys
import time

import numpy as np
import torch
from sentence_transformers import SentenceTransformer
import sentence_transformers as st_pkg

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
CORPUS_PATH = SCRIPT_DIR.parent / "fixtures" / "corpus.jsonl"
OUT_DIR = SCRIPT_DIR.parent / "out"

MODELS = {
    "granite": "ibm-granite/granite-embedding-small-english-r2",
    "bge": "BAAI/bge-small-en-v1.5",
}

BATCH_SIZE = 8
NUM_THREADS = 8


def load_corpus(path: pathlib.Path):
    ids = []
    texts = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            ids.append(rec["id"])
            texts.append(rec["text"])
    return ids, texts


def describe_pooling(model: SentenceTransformer) -> str:
    """Summarize the model's 1_Pooling/config.json in one string."""
    try:
        pooling_module = None
        for module in model._modules.values():
            if module.__class__.__name__ == "Pooling":
                pooling_module = module
                break
        if pooling_module is None:
            return "unknown (no Pooling module found in modules chain)"
        cfg = pooling_module.get_config_dict()
        active_modes = [k for k, v in cfg.items() if k.startswith("pooling_mode_") and v]
        return f"{active_modes} (raw config: {cfg})"
    except Exception as exc:  # noqa: BLE001
        return f"unknown (error introspecting pooling module: {exc!r})"


def encode_model(short_name: str, model_id: str, ids, texts):
    print(f"\n=== Loading {short_name} ({model_id}) ===", flush=True)
    t0 = time.time()
    # Force float32 compute explicitly. Some checkpoints (e.g. granite-embedding-
    # small-english-r2) declare torch_dtype: bfloat16 in their HF config.json, and
    # sentence-transformers respects that by default -- silently running the model
    # in bfloat16 even though outputs are later cast to float32 for storage. That
    # produces float32-*shaped* but bfloat16-*precision* vectors (visible as
    # normalized-embedding norms drifting up to ~0.5% from 1.0 instead of being
    # numerically ~1.0), which would corrupt this float32 parity reference. Pin
    # float32 compute for every model regardless of the checkpoint's stored dtype.
    model = SentenceTransformer(model_id, device="cpu", model_kwargs={"torch_dtype": "float32"})
    model.eval()
    print(f"Loaded in {time.time() - t0:.1f}s", flush=True)

    max_seq_length = model.max_seq_length
    pooling_summary = describe_pooling(model)
    print(f"max_seq_length={max_seq_length}", flush=True)
    print(f"pooling={pooling_summary}", flush=True)

    with torch.inference_mode():
        t0 = time.time()
        embeddings = model.encode(
            texts,
            batch_size=BATCH_SIZE,
            show_progress_bar=True,
            convert_to_numpy=True,
            normalize_embeddings=True,
        )
        elapsed = time.time() - t0
    print(f"Encoded {len(texts)} texts in {elapsed:.1f}s", flush=True)

    embeddings = embeddings.astype(np.float32)
    dims = embeddings.shape[1]

    vectors = {}
    for id_, vec in zip(ids, embeddings):
        vectors[id_] = [float(x) for x in vec]

    result = {
        "model": model_id,
        "impl": "sentence-transformers",
        "dims": dims,
        "st_version": st_pkg.__version__,
        "torch_version": torch.__version__,
        "max_seq_length": max_seq_length,
        "pooling": pooling_summary,
        "vectors": vectors,
    }
    return result


def main():
    torch.set_num_threads(NUM_THREADS)
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    if not CORPUS_PATH.exists():
        print(f"ERROR: corpus not found at {CORPUS_PATH}", file=sys.stderr)
        sys.exit(1)

    ids, texts = load_corpus(CORPUS_PATH)
    print(f"Loaded corpus: {len(ids)} texts from {CORPUS_PATH}", flush=True)

    for short_name, model_id in MODELS.items():
        try:
            result = encode_model(short_name, model_id, ids, texts)
        except Exception as exc:  # noqa: BLE001
            print(f"\nFATAL: failed to load/encode model {short_name} ({model_id}): {exc!r}",
                  file=sys.stderr)
            raise

        out_path = OUT_DIR / f"reference_{short_name}.json"
        with out_path.open("w", encoding="utf-8") as f:
            json.dump(result, f)
        print(f"Wrote {out_path} ({len(result['vectors'])} vectors, dims={result['dims']})",
              flush=True)


if __name__ == "__main__":
    main()
