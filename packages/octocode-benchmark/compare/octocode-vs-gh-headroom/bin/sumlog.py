#!/usr/bin/env python3
"""Total the measured chars for one arm-A run.

    python3 bin/sumlog.py run.jsonl

Prints raw chars, compressed chars (the number that enters context), and the
overall reduction. Use the compressed total as arm A's "chars in" in SCORING.md.
"""
import json
import sys

raw = out = 0
calls = 0
path = sys.argv[1] if len(sys.argv) > 1 else "run.jsonl"
with open(path) as fh:
    for line in fh:
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        raw += int(rec.get("raw_chars", 0))
        out += int(rec.get("out_chars", 0))
        calls += 1

pct = (1 - out / raw) * 100 if raw else 0.0
print(f"calls={calls}  raw_chars={raw}  chars_in={out}  reduction={pct:.1f}%")
