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
import hashlib
import json
import os
from pathlib import Path
import sys
import time

from headroom import compress
from headroom.compress import CompressConfig
from headroom.transforms.kompress_compressor import KompressCompressor

raw = sys.stdin.read()
cfg = CompressConfig(compress_user_messages=True, protect_recent=0)
model_backend = None

# Each ghc call is a fresh process, while Headroom's ready cache is process-local.
# Preload the already-downloaded model in every process before routing. Even a
# compact JSON response can contain a field that enters the mixed compressor;
# no content/size heuristic can prove that the neural branch will be skipped.
# Headroom's ready cache is process-local, so an earlier process is insufficient.
try:
    model_backend = KompressCompressor().preload(allow_download=False)
except Exception as exc:
    raise RuntimeError(
        "Kompress is not ready in this process; run bin/preflight.py --warmup"
    ) from exc

result = compress([{"role": "user", "content": raw}], config=cfg)

out = result.messages[-1]["content"]
if not isinstance(out, str):
    out = json.dumps(out)

transforms = list(result.transforms_applied)
artifact_id = f"{time.time_ns()}-{os.getpid()}"
artifact_dir_value = os.environ.get("HR_ARTIFACT_DIR")
log_value = os.environ.get("HR_LOG")
raw_artifact = None
out_artifact = None

if artifact_dir_value:
    artifact_dir = Path(artifact_dir_value)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    raw_path = artifact_dir / f"{artifact_id}.raw.txt"
    out_path = artifact_dir / f"{artifact_id}.out.txt"
    raw_path.write_text(raw, encoding="utf-8")
    out_path.write_text(out, encoding="utf-8")
    raw_artifact = str(raw_path)
    out_artifact = str(out_path)

command_text = os.environ.get("HR_COMMAND", "headroom:stdin")
model_out_chars = len(command_text)
model_in_chars = len(out)

record = {
    "cmd": command_text,
    "source_exit_code": int(os.environ.get("HR_SOURCE_EXIT_CODE", "0")),
    "char_unit": "unicode_code_points",
    "raw_chars": len(raw),
    "out_chars": len(out),
    # directional accounting: model-out (command) + model-in (compressed output)
    "model_out_chars": model_out_chars,
    "model_in_chars": model_in_chars,
    "total_chars": model_out_chars + model_in_chars,
    "raw_bytes": len(raw.encode("utf-8")),
    "out_bytes": len(out.encode("utf-8")),
    "raw_sha256": hashlib.sha256(raw.encode("utf-8")).hexdigest(),
    "out_sha256": hashlib.sha256(out.encode("utf-8")).hexdigest(),
    "tokens_before": result.tokens_before,
    "tokens_after": result.tokens_after,
    "model_backend": model_backend,
    "ratio": result.compression_ratio,
    "transforms": transforms,
    "raw_artifact": raw_artifact,
    "out_artifact": out_artifact,
}

if log_value:
    log_path = Path(log_value)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")

sys.stdout.write(out)
sys.stderr.write("HEADROOM_MEASUREMENT=" + json.dumps(record, ensure_ascii=False) + "\n")
