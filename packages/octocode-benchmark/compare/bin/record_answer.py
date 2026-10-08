#!/usr/bin/env python3
"""Record the model's final answer as measured model-out characters.

Tool calls capture the command the model wrote (model-out) and the output it
read (model-in). The final answer the model produces is also chars through the
model — pure model-out — and must enter the total the same way. Log it as one
record so sumlog.py folds it into model_out_chars / total_chars.

    python3 bin/record_answer.py --log run.jsonl --question Q1 --file answer.txt
    printf '%s' "$ANSWER" | python3 bin/record_answer.py --log run.jsonl --question Q1
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log", required=True)
    parser.add_argument("--question", required=True)
    parser.add_argument("--file", help="answer text file; omit to read stdin")
    args = parser.parse_args()

    if args.file:
        text = Path(args.file).read_text(encoding="utf-8")
    else:
        text = sys.stdin.read()

    answer_chars = len(text)
    record = {
        "kind": "answer",
        "question": args.question,
        "cmd": f"answer:{args.question}",
        "char_unit": "unicode_code_points",
        # an answer is pure model-out; it reads nothing back into context
        "model_out_chars": answer_chars,
        "model_in_chars": 0,
        "total_chars": answer_chars,
        "sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
    }

    log = Path(args.log)
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")

    print(json.dumps({"question": args.question, "answer_chars": answer_chars}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
