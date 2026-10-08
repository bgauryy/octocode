#!/usr/bin/env python3
"""Validate and total one instrumented benchmark log.

    python3 bin/sumlog.py run.jsonl --strict --diagnostics run-diagnostics.log

Prints raw chars, compressed chars (the number that enters context), and the
overall reduction. Use the compressed total as arm A's "chars in" in SCORING.md.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
from collections import Counter

# Advisory fairness heuristics. The "leanest legal path" rule is otherwise
# judge-enforced; these give a mechanical, reviewable signal so a wasteful path
# (whole-tree dump / oversized single read) cannot pass unnoticed. They WARN,
# never fail — some questions legitimately need a broad read.
WHOLE_TREE_RE = re.compile(r"recursive=1\b")
LARGE_READ_CHARS = 40000


def fairness_notices(label: str, model_in_chars: int) -> list[str]:
    notices: list[str] = []
    if WHOLE_TREE_RE.search(label):
        notices.append(f"whole-tree dump (recursive=1): {label!r}")
    if model_in_chars >= LARGE_READ_CHARS:
        notices.append(
            f"large single read ({model_in_chars} chars ≥ {LARGE_READ_CHARS}): {label!r}"
        )
    return notices


def digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", nargs="?", default="run.jsonl")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--diagnostics")
    args = parser.parse_args()

    raw = out = calls = failed_calls = 0
    model_in = model_out = 0
    answers = 0
    errors: list[str] = []
    fairness: list[str] = []
    transforms: Counter[str] = Counter()
    with Path(args.path).open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            is_answer = record.get("kind") == "answer"
            is_headroom = "raw_chars" in record

            # Directional totals: ALL chars through the model = in + out.
            # model_in defaults to the legacy output-in field when absent.
            default_in = int(record.get("out_chars", record.get("chars", 0)))
            model_in += int(record.get("model_in_chars", default_in))
            model_out += int(record.get("model_out_chars", 0))

            if is_answer:
                answers += 1
                continue

            if is_headroom:
                raw += int(record.get("raw_chars", 0))
                out += int(record.get("out_chars", 0))
                if int(record.get("source_exit_code", -1)) != 0:
                    failed_calls += 1
            else:
                raw += int(record.get("chars", 0))
                out += int(record.get("chars", 0))
                if int(record.get("exit_code", -1)) != 0:
                    failed_calls += 1
            calls += 1
            fairness.extend(
                fairness_notices(
                    str(record.get("cmd", "")),
                    int(record.get("model_in_chars", record.get("chars", 0))),
                )
            )
            for transform in record.get("transforms", []):
                transforms[str(transform)] += 1

            if not args.strict:
                continue
            prefix = f"line {line_number}"
            if record.get("char_unit") != "unicode_code_points":
                errors.append(f"{prefix}: wrong/missing character unit")
            if is_headroom:
                if "source_exit_code" not in record:
                    errors.append(f"{prefix}: missing source exit status")
                if not record.get("transforms"):
                    errors.append(f"{prefix}: missing transforms")
                if "router:protected:user_message" in record.get("transforms", []):
                    errors.append(f"{prefix}: protected user message")
                artifact_fields = (
                    ("raw_artifact", "raw_chars", "raw_sha256"),
                    ("out_artifact", "out_chars", "out_sha256"),
                )
            else:
                artifact_fields = (("artifact", "chars", "sha256"),)
            for field, chars_field, hash_field in artifact_fields:
                artifact_value = record.get(field)
                if not artifact_value or not Path(str(artifact_value)).is_file():
                    errors.append(f"{prefix}: missing {field}")
                    continue
                # Read without newline translation so CRLF (\r\n) survives — universal
                # newlines would strip \r and false-fail the char/hash check on CRLF files.
                with open(str(artifact_value), encoding="utf-8", newline="") as artifact_handle:
                    text = artifact_handle.read()
                if len(text) != int(record.get(chars_field, -1)):
                    errors.append(f"{prefix}: {field} character count mismatch")
                if digest(text) != record.get(hash_field):
                    errors.append(f"{prefix}: {field} hash mismatch")

    if args.diagnostics and Path(args.diagnostics).is_file():
        diagnostics = Path(args.diagnostics).read_text(encoding="utf-8")
        if "Kompress model not ready" in diagnostics:
            errors.append("diagnostics: Kompress model not ready")
        if "ML compression DISABLED" in diagnostics:
            errors.append("diagnostics: ML compression disabled")

    reduction = (1 - out / raw) * 100 if raw else 0.0
    total_chars = model_in + model_out
    print(
        f"calls={calls}  raw_chars={raw}  chars_in={out}  "
        f"reduction={reduction:.1f}%  failed_calls={failed_calls}  "
        f"transforms={dict(transforms)}"
    )
    print(
        f"model_in_chars={model_in}  model_out_chars={model_out}  "
        f"total_chars={total_chars}  answers={answers}"
    )
    for notice in fairness:
        print(f"FAIRNESS: {notice}")
    for error in errors:
        print(f"ERROR: {error}")
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
