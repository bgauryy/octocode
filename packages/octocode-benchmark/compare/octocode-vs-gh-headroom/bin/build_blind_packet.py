#!/usr/bin/env python3
"""Build shuffled, tool-redacted answer pairs for the blind grader."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re


QUESTION = re.compile(r"^## Q(\d+)\b.*$", re.MULTILINE)
IDENTITY = re.compile(
    r"(?:npx\s+octocode|octocode|headroom|\./bin/ghc|\bgh[A-Z][A-Za-z]+|\bgh\s+)",
    re.IGNORECASE,
)


def sections(path: Path) -> dict[int, str]:
    text = path.read_text(encoding="utf-8")
    matches = list(QUESTION.finditer(text))
    result: dict[int, str] = {}
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        body = text[match.end() : end].strip()
        cleaned: list[str] = []
        for line in body.splitlines():
            if re.match(r"^\s*\d+\.\s+`", line):
                continue
            if re.match(r"^\s*\*\*(?:Instrumentation|Measure|Calls?)\b", line):
                continue
            line = re.sub(r"\s*\*\*(?:Metrics|Calls):\*\*.*$", "", line)
            line = re.sub(r"\s+Commands?:.*$", "", line)
            line = IDENTITY.sub("[research tool]", line)
            cleaned.append(line.rstrip())
        result[int(match.group(1))] = "\n".join(cleaned).strip()
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("campaign_dir", type=Path)
    parser.add_argument("metrics", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--question-count", type=int, default=20)
    args = parser.parse_args()
    campaign = args.campaign_dir.resolve()
    metrics = json.loads(args.metrics.read_text(encoding="utf-8"))["summary"]
    mappings = {
        1: (("X", "headroom"), ("Y", "octocode")),
        2: (("X", "octocode"), ("Y", "headroom")),
        3: (("X", "headroom"), ("Y", "octocode")),
    }
    answer_cache = {
        (arm, pass_number): sections(
            campaign / "answers" / f"{arm}-p{pass_number}.md"
        )
        for pass_number in range(1, 4)
        for arm in ("headroom", "octocode")
    }

    output = [
        "# Blind grading packet",
        "",
        "Tool identities and command names are redacted. X/Y order changes by pass. "
        "Metrics are authoritative artifact-backed Unicode counts.",
    ]
    for pass_number, pairs in mappings.items():
        output.extend(["", f"## Pass {pass_number}"])
        for question_number in range(1, args.question_count + 1):
            output.extend(["", f"### Q{question_number}"])
            for label, arm in pairs:
                question_metrics = metrics[arm][f"pass_{pass_number}"]["questions"][
                    f"Q{question_number}"
                ]
                output.extend(
                    [
                        "",
                        f"#### {label}",
                        "",
                        answer_cache[(arm, pass_number)][question_number],
                        "",
                        (
                            f"Authoritative metrics: calls={question_metrics['calls']}, "
                            f"chars_in={question_metrics['chars_in']}, "
                            f"failed_calls={question_metrics['failed_calls']}."
                        ),
                    ]
                )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(output) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
