#!/usr/bin/env python3
"""Deterministically generate the S1 parity fixture corpus.

100 texts of varied length built from this repo's markdown prose:
  - t001 is the kernel-proof Mastra memory verbatim
  - ~80 short/medium memory-like texts (1-8 sentences)
  - 14 long texts (> 512 tokens)
  - 5 very long texts (> 4096 tokens)

Parity testing only requires that both embedding implementations see
identical bytes; semantic realism is secondary. Regenerating this file
must be byte-stable (fixed seed, sorted inputs).
"""
import json
import random
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / "corpus.jsonl"

SOURCES = sorted(
    p for p in REPO.glob("*.md")
)

sentences = []
for p in SOURCES:
    text = p.read_text(encoding="utf-8")
    # strip code fences, tables, headers, list markers
    text = re.sub(r"```.*?```", " ", text, flags=re.S)
    text = re.sub(r"^\|.*$", " ", text, flags=re.M)
    text = re.sub(r"^#.*$", " ", text, flags=re.M)
    text = re.sub(r"^[-*] ", "", text, flags=re.M)
    text = re.sub(r"[`*_>\[\]()|]", " ", text)
    text = re.sub(r"\s+", " ", text)
    for s in re.split(r"(?<=[.!?]) ", text):
        s = s.strip()
        if 30 <= len(s) <= 400:
            sentences.append(s)

assert len(sentences) >= 120, f"only {len(sentences)} sentences harvested"

rng = random.Random(20260831)
records = []

records.append({
    "id": "t001",
    "text": "We rejected Mastra because suspend/resume durability violated the Factory invariants.",
})

def compose(n_sentences):
    return " ".join(rng.choice(sentences) for _ in range(n_sentences))

for i in range(2, 82):  # t002..t081: short/medium
    records.append({"id": f"t{i:03d}", "text": compose(rng.randint(1, 8))})

for i in range(82, 96):  # t082..t095: long (>512 tokens ~ >400 words)
    records.append({"id": f"t{i:03d}", "text": compose(rng.randint(60, 140))})

for i in range(96, 101):  # t096..t100: very long (>4096 tokens ~ >3500 words)
    records.append({"id": f"t{i:03d}", "text": compose(rng.randint(320, 420))})

with OUT.open("w", encoding="utf-8") as f:
    for r in records:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")

words = [len(r["text"].split()) for r in records]
print(f"wrote {len(records)} texts to {OUT}")
print(f"word counts: min={min(words)} median={sorted(words)[50]} max={max(words)}")
