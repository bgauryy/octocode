#!/usr/bin/env python3
"""Classify benchmark tool calls without erasing failed or empty predecessors."""

from __future__ import annotations

from collections import Counter, defaultdict
import json
import re
from typing import Any


OUTCOMES = (
    "first-valid", "schema-invalid", "runtime-failed", "productive-success",
    "expected-empty", "unproductive-empty",
)
EMPTY_CLASSES = ("expected-absence", "scope-empty", "provider-incomplete", "query-miss")
SCHEMA_ERROR = re.compile(
    r"unknown field|unrecognized (argument|key)|schema|validation|invalid (input|argument)|did you mean",
    re.IGNORECASE,
)


def _output_text(record: dict[str, Any]) -> str:
    value = record.get("output", record.get("response", record.get("stderr", "")))
    return json.dumps(value, sort_keys=True) if not isinstance(value, str) else value


def classify_call(record: dict[str, Any]) -> str:
    declared = record.get("call_outcome")
    if declared in OUTCOMES:
        return str(declared)
    exit_code = int(record.get("exit_code", record.get("source_exit_code", 0)))
    if exit_code != 0:
        return "schema-invalid" if SCHEMA_ERROR.search(_output_text(record)) else "runtime-failed"
    result_count = record.get("result_count")
    status = record.get("status")
    if result_count == 0 or status == "empty":
        return "expected-empty" if record.get("empty_classification") == "expected-absence" else "unproductive-empty"
    return "first-valid" if int(record.get("attempt_index", 1)) == 1 else "productive-success"


def validate_trajectory(records: list[dict[str, Any]]) -> list[str]:
    errors: list[str] = []
    seen_attempts: defaultdict[str, set[int]] = defaultdict(set)
    for index, record in enumerate(records):
        logical_id = record.get("logical_call_id")
        if not isinstance(logical_id, str) or not logical_id:
            errors.append(f"record {index} lacks logical_call_id")
            continue
        attempt = record.get("attempt_index", 1)
        if not isinstance(attempt, int) or attempt < 1:
            errors.append(f"record {index} has invalid attempt_index")
            continue
        if attempt in seen_attempts[logical_id]:
            errors.append(f"duplicate attempt {attempt} for {logical_id}")
        seen_attempts[logical_id].add(attempt)
        empty_class = record.get("empty_classification")
        if empty_class is not None and empty_class not in EMPTY_CLASSES:
            errors.append(f"record {index} has invalid empty classification")
        outcome = classify_call(record)
        if outcome in {"expected-empty", "unproductive-empty"} and empty_class not in EMPTY_CLASSES:
            errors.append(f"record {index} is empty without a typed empty classification")
    for logical_id, attempts in seen_attempts.items():
        if attempts != set(range(1, max(attempts) + 1)):
            errors.append(f"attempt sequence for {logical_id} is not contiguous")
    return errors


def summarize_trajectory(records: list[dict[str, Any]]) -> dict[str, object]:
    counts = Counter(classify_call(record) for record in records)
    by_logical: defaultdict[str, list[tuple[int, str]]] = defaultdict(list)
    for record in records:
        by_logical[str(record.get("logical_call_id", ""))].append(
            (int(record.get("attempt_index", 1)), classify_call(record))
        )
    repaired = 0
    for attempts in by_logical.values():
        ordered = [outcome for _, outcome in sorted(attempts)]
        if "schema-invalid" in ordered and any(outcome in {"productive-success", "first-valid", "expected-empty"} for outcome in ordered[1:]):
            repaired += 1
    total = len(records)
    return {
        "totalCalls": total,
        "firstValidCalls": counts["first-valid"],
        "schemaInvalidCalls": counts["schema-invalid"],
        "runtimeFailedCalls": counts["runtime-failed"],
        "productiveSuccessCalls": counts["productive-success"],
        "expectedEmptyCalls": counts["expected-empty"],
        "unproductiveEmptyCalls": counts["unproductive-empty"],
        "repairedSchemaCalls": repaired,
        "schemaInvalidRate": counts["schema-invalid"] / total if total else 0.0,
        "unproductiveEmptyRate": counts["unproductive-empty"] / total if total else 0.0,
    }
