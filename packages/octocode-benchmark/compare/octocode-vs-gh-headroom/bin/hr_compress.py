#!/usr/bin/env python3
"""Compress one gh command's output through Headroom (benchmark arm A).

Reads raw text on stdin, writes the compressed text to stdout, and writes ONE
measurement line to stderr:

    raw_chars=<n> out_chars=<n> tokens_before=<n> tokens_after=<n> ratio=<f> transforms=[...]

Why the explicit config (this is the whole trick — do not remove it):
  * compress_user_messages=True — the library treats a lone user message as the
    *prompt* and PROTECTS it (ratio 0.0). gh output arrives as a user message,
    so we must opt it in. The upstream docstring says exactly this: "Set True …
    when user messages contain large tool outputs."
  * protect_recent=0 — otherwise the last 4 messages (i.e. our only message) are
    held out as "active conversation" and never compressed.

One-shot, non-reversible: this does NOT run the proxy or the CCR retrieve loop,
so the chars written here are the chars that actually enter the agent's context.
That keeps SCORING.md's "chars in" honest — there is no hidden re-expansion.

Run it with Headroom's own interpreter (the uv-tool venv), not system python:
    HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"
    gh ... | "$HR_PY" hr_compress.py
"""
import json
import sys

from headroom import compress
from headroom.compress import CompressConfig

raw = sys.stdin.read()
cfg = CompressConfig(compress_user_messages=True, protect_recent=0)
result = compress([{"role": "user", "content": raw}], config=cfg)

out = result.messages[-1]["content"]
if not isinstance(out, str):
    out = json.dumps(out)

sys.stdout.write(out)
sys.stderr.write(
    f"raw_chars={len(raw)} out_chars={len(out)} "
    f"tokens_before={result.tokens_before} tokens_after={result.tokens_after} "
    f"ratio={result.compression_ratio:.3f} transforms={result.transforms_applied}\n"
)
