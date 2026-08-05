#!/usr/bin/env python3
"""Run one command, preserve its complete output, and log exact character metrics."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log", required=True)
    parser.add_argument("--artifact-dir", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required after --")

    started = time.perf_counter()
    process = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    elapsed_ms = round((time.perf_counter() - started) * 1000, 3)
    raw = process.stdout
    text = raw.decode("utf-8", errors="replace")

    artifact_dir = Path(args.artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    artifact_id = f"{time.time_ns()}-{process.pid if hasattr(process, 'pid') else 'command'}"
    artifact = artifact_dir / f"{artifact_id}.txt"
    artifact.write_text(text, encoding="utf-8")

    record = {
        "cmd": args.label,
        "argv": command,
        "char_unit": "unicode_code_points",
        "chars": len(text),
        "bytes": len(raw),
        "sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "exit_code": process.returncode,
        "elapsed_ms": elapsed_ms,
        "artifact": str(artifact),
    }
    log = Path(args.log)
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")

    sys.stdout.write(text)
    return process.returncode


if __name__ == "__main__":
    raise SystemExit(main())
