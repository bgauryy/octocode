#!/usr/bin/env python3
"""Deterministic validity gate for the Headroom benchmark instrumentation."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


HERE = Path(__file__).resolve().parent


def validate_record(record: dict[str, object]) -> list[str]:
    errors: list[str] = []
    if record.get("char_unit") != "unicode_code_points":
        errors.append("character unit is not unicode_code_points")
    transforms = record.get("transforms")
    if not isinstance(transforms, list) or not transforms:
        errors.append("missing transform classification")
    elif "router:protected:user_message" in transforms:
        errors.append("user message remained protected")
    for key in ("raw_artifact", "out_artifact"):
        value = record.get(key)
        if not isinstance(value, str) or not Path(value).is_file():
            errors.append(f"missing {key}")
    return errors


def run_fixture(hr_python: str, name: str, text: str) -> tuple[dict[str, object], str]:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        log = tmp_path / f"{name}.jsonl"
        env = {
            **os.environ,
            "HR_LOG": str(log),
            "HR_ARTIFACT_DIR": str(tmp_path / "artifacts"),
            "HR_COMMAND": f"fixture:{name}",
        }
        result = subprocess.run(
            [hr_python, str(HERE / "hr_compress.py")],
            input=text,
            text=True,
            capture_output=True,
            env=env,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stdout + result.stderr)
        record = json.loads(log.read_text(encoding="utf-8").strip())
        # Copy the artifact-existence result into the returned record before the
        # temporary directory is removed.
        record["artifact_errors"] = validate_record(record)
        return record, result.stderr


def self_test() -> int:
    base = {
        "char_unit": "unicode_code_points",
        "transforms": ["router:noop"],
        "raw_artifact": __file__,
        "out_artifact": __file__,
    }
    assert validate_record(base) == []
    assert "missing transform classification" in validate_record({**base, "transforms": []})
    assert "user message remained protected" in validate_record(
        {**base, "transforms": ["router:protected:user_message"]}
    )
    print("preflight self-test: PASS")
    return 0


def warmup(hr_python: str) -> None:
    code = (
        "from headroom.transforms.kompress_compressor import KompressCompressor; "
        "print(KompressCompressor().preload(allow_download=True))"
    )
    subprocess.run([hr_python, "-c", code], check=True)


def live(hr_python: str, do_warmup: bool) -> int:
    if do_warmup:
        warmup(hr_python)

    fixtures = {
        "short_noop": "short input",
        "structured_json": json.dumps(
            [{"number": i, "title": f"item-{i}", "state": "open"} for i in range(1200)]
        ),
        "structured_mixed": json.dumps(
            [
                {
                    "path": f"src/module-{i}.ts",
                    "textMatches": [
                        {
                            "fragment": (
                                "export function validateRequest(input) { "
                                "return input != null && input.state === 'ready'; } "
                                "This result is documented for callers and includes context."
                            )
                        }
                    ],
                }
                for i in range(160)
            ]
        ),
        "long_prose": " ".join(
            f"Request {i} completed after validation and returned a documented response."
            for i in range(1200)
        ),
    }
    failures: list[str] = []
    summary: dict[str, object] = {}
    for name, text in fixtures.items():
        record, stderr = run_fixture(hr_python, name, text)
        summary[name] = {
            "raw_chars": record["raw_chars"],
            "out_chars": record["out_chars"],
            "transforms": record["transforms"],
            "model_backend": record.get("model_backend"),
        }
        failures.extend(f"{name}: {error}" for error in record["artifact_errors"])
        if "Kompress model not ready" in stderr:
            failures.append(f"{name}: Kompress model not ready")

    json_transforms = summary["structured_json"]["transforms"]
    if not any("smart_crusher" in item for item in json_transforms):
        failures.append("structured_json: SmartCrusher did not run")

    mixed = summary["structured_mixed"]
    if not mixed.get("model_backend"):
        failures.append("structured_mixed: Kompress model was not loaded in-process")
    if not any("mixed" in item for item in mixed["transforms"]):
        failures.append("structured_mixed: mixed compressor did not run")

    prose = summary["long_prose"]
    if not prose.get("model_backend"):
        failures.append("long_prose: Kompress model was not loaded in-process")
    if int(prose["out_chars"]) >= int(prose["raw_chars"]):
        failures.append("long_prose: model loaded but content was not compressed")

    print(json.dumps({"fixtures": summary, "failures": failures}, indent=2))
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--warmup", action="store_true")
    parser.add_argument(
        "--headroom-python",
        default=os.environ.get(
            "HR_PY",
            str(Path.home() / ".local/share/uv/tools/headroom-ai/bin/python"),
        ),
    )
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    return live(args.headroom_python, args.warmup)


if __name__ == "__main__":
    raise SystemExit(main())
