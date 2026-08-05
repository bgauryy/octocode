#!/usr/bin/env python3
"""Strictly validate and summarize every artifact in a benchmark campaign."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import subprocess
import sys


HERE = Path(__file__).resolve().parent
QUESTION_HEADING = re.compile(r"^## Q(\d+)\b", re.MULTILINE)


def read_metrics(path: Path) -> dict[str, object]:
    calls = raw = chars = failed_calls = 0
    transforms: Counter[str] = Counter()
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            record = json.loads(line)
            calls += 1
            if "raw_chars" in record:
                raw += int(record["raw_chars"])
                chars += int(record["out_chars"])
                if int(record.get("source_exit_code", -1)) != 0:
                    failed_calls += 1
            else:
                raw += int(record["chars"])
                chars += int(record["chars"])
                if int(record.get("exit_code", -1)) != 0:
                    failed_calls += 1
            transforms.update(str(item) for item in record.get("transforms", []))
    return {
        "calls": calls,
        "raw_chars": raw,
        "chars_in": chars,
        "failed_calls": failed_calls,
        "transforms": dict(transforms),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("campaign_dir", type=Path)
    parser.add_argument("--question-count", type=int, default=20)
    args = parser.parse_args()
    campaign = args.campaign_dir.resolve()
    expected_questions = list(range(1, args.question_count + 1))
    failures: list[str] = []
    summary: dict[str, object] = {}

    for arm in ("headroom", "octocode"):
        arm_summary: dict[str, object] = {}
        for pass_number in range(1, 4):
            questions: dict[str, object] = {}
            totals: Counter[str] = Counter()
            answer_path = campaign / "answers" / f"{arm}-p{pass_number}.md"
            if not answer_path.is_file() or not answer_path.read_text(encoding="utf-8").strip():
                failures.append(f"missing answer: {answer_path}")
            else:
                headings = [
                    int(value)
                    for value in QUESTION_HEADING.findall(
                        answer_path.read_text(encoding="utf-8")
                    )
                ]
                if headings != expected_questions:
                    failures.append(
                        f"answer headings mismatch: {answer_path}: {headings}"
                    )

            for question_number in expected_questions:
                stem = f"{arm}-p{pass_number}-Q{question_number}"
                log = campaign / f"{stem}.jsonl"
                if not log.is_file() or not log.read_text(encoding="utf-8").strip():
                    failures.append(f"missing log: {log}")
                    continue
                command = [sys.executable, str(HERE / "sumlog.py"), str(log), "--strict"]
                if arm == "headroom":
                    command.extend(["--diagnostics", str(campaign / f"{stem}-diagnostics.log")])
                validation = subprocess.run(command, text=True, capture_output=True)
                if validation.returncode != 0:
                    failures.append(f"invalid {stem}: {validation.stdout}{validation.stderr}".strip())
                    continue
                metrics = read_metrics(log)
                questions[f"Q{question_number}"] = metrics
                totals.update(
                    {
                        "calls": int(metrics["calls"]),
                        "raw_chars": int(metrics["raw_chars"]),
                        "chars_in": int(metrics["chars_in"]),
                        "failed_calls": int(metrics["failed_calls"]),
                    }
                )
            arm_summary[f"pass_{pass_number}"] = {
                "questions": questions,
                "totals": dict(totals),
            }
        summary[arm] = arm_summary

    result = {"campaign": str(campaign), "summary": summary, "failures": failures}
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
