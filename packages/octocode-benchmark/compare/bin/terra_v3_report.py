#!/usr/bin/env python3
"""Aggregate a validated Terra v3 campaign without conflating metric families."""

from __future__ import annotations

import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import statistics
from typing import Iterable

from terra_v3_contracts import (
    load_and_validate_contracts,
    validate_campaign_manifest,
    validate_suite_contract,
)


TOKEN_FIELDS = ("inputTokens", "outputTokens", "totalTokens", "cachedInputTokens", "reasoningTokens")
METRIC_FIELDS = (
    "wallMs",
    "cpuUserMs",
    "cpuSystemMs",
    "peakRssBytes",
    "cgroupPeakBytes",
    "ioReadBytes",
    "ioWriteBytes",
    "contextCharacters",
)
TRAJECTORY_FIELDS = (
    "totalCalls",
    "firstValidCalls",
    "schemaInvalidCalls",
    "runtimeFailedCalls",
    "productiveSuccessCalls",
    "expectedEmptyCalls",
    "unproductiveEmptyCalls",
    "repairedSchemaCalls",
)
SCORE_FIELDS = ("correctness", "depth", "workflow")


def _number(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
        raise ValueError(f"{name} must be a nonnegative number")
    return float(value)


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"{name} must be a nonnegative integer")
    return value


def _summary(values: Iterable[float]) -> dict[str, float | int]:
    observed = sorted(values)
    if not observed:
        return {"count": 0}
    rank = max(0, math.ceil(0.95 * len(observed)) - 1)
    return {
        "count": len(observed),
        "sum": sum(observed),
        "mean": statistics.fmean(observed),
        "median": statistics.median(observed),
        "p95": observed[rank],
        "min": observed[0],
        "max": observed[-1],
    }


def _clean_numbers(summary: dict[str, float | int]) -> dict[str, float | int]:
    return {
        key: int(value) if isinstance(value, float) and value.is_integer() and key in {"sum", "min", "max"} else value
        for key, value in summary.items()
    }


def _validate_token_usage(run: dict[str, object]) -> dict[str, int]:
    usage = run.get("tokenUsage")
    if not isinstance(usage, dict) or usage.get("source") != "provider":
        raise ValueError("token usage must be provider-reported, never estimated from characters")
    normalized = {
        field: _integer(usage.get(field, 0), f"tokenUsage.{field}")
        for field in TOKEN_FIELDS
    }
    if normalized["totalTokens"] != normalized["inputTokens"] + normalized["outputTokens"]:
        raise ValueError("tokenUsage.totalTokens must equal inputTokens + outputTokens")
    if normalized["cachedInputTokens"] > normalized["inputTokens"]:
        raise ValueError("cachedInputTokens cannot exceed inputTokens")
    if normalized["reasoningTokens"] > normalized["outputTokens"]:
        raise ValueError("reasoningTokens cannot exceed outputTokens")
    return normalized


def _judge_scores(
    manifest: dict[str, object], collection: str = "judges",
) -> dict[str, dict[str, list[float]]]:
    assignments: dict[tuple[object, object, object], dict[str, str]] = {}
    for item in manifest.get("blindAssignments", []):
        if not isinstance(item, dict) or not isinstance(item.get("labelToArm"), dict):
            raise ValueError("blindAssignments entries must contain a labelToArm mapping")
        key = (item.get("judgeId"), item.get("caseId"), item.get("pass"))
        if key in assignments:
            raise ValueError(f"duplicate blind assignment: {key}")
        labels = item["labelToArm"]
        if set(labels) != {"X", "Y"} or len(set(labels.values())) != 2:
            raise ValueError(f"invalid blind labels for {key}")
        assignments[key] = labels

    by_arm: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    for judge in manifest.get(collection, []):
        if not isinstance(judge, dict) or "labelToArm" in judge:
            raise ValueError("judge records must remain blind and cannot contain labelToArm")
        key = (judge.get("judgeId"), judge.get("caseId"), judge.get("pass"))
        labels = assignments.get(key)
        scores = judge.get("scores")
        if labels is None or not isinstance(scores, dict) or set(scores) != {"X", "Y"}:
            raise ValueError(f"judge record lacks a matching blind assignment: {key}")
        for label, arm in labels.items():
            score = scores.get(label)
            if not isinstance(score, dict):
                raise ValueError(f"judge score {key}/{label} must be an object")
            for field in SCORE_FIELDS:
                value = _number(score.get(field), f"judge.{field}")
                lower, upper = (0, 10) if field == "correctness" else (1, 5)
                if not lower <= value <= upper:
                    raise ValueError(f"judge.{field} must be within {lower}..{upper}")
                by_arm[arm][field].append(value)
    return by_arm


def _geometric_ratio_summary(values: Iterable[float]) -> dict[str, float | int | None]:
    observed = [value for value in values if value > 0]
    if not observed:
        return {"count": 0, "geometricMean": None, "ci95Low": None, "ci95High": None}
    logs = [math.log(value) for value in observed]
    mean = statistics.fmean(logs)
    if len(logs) < 2:
        low = high = mean
    else:
        margin = 1.96 * statistics.stdev(logs) / math.sqrt(len(logs))
        low, high = mean - margin, mean + margin
    return {
        "count": len(logs),
        "geometricMean": math.exp(mean),
        "ci95Low": math.exp(low),
        "ci95High": math.exp(high),
    }


def build_comparison(manifest: dict[str, object]) -> dict[str, object]:
    runs = manifest.get("runs")
    if not isinstance(runs, list) or not runs:
        raise ValueError("campaign must contain runs")
    judge_scores = _judge_scores(manifest)
    confirmation_scores = _judge_scores(manifest, "confirmationJudges")
    grouped: dict[str, list[dict[str, object]]] = defaultdict(list)
    pair_index: dict[tuple[object, object, object, str, str], dict[str, float]] = defaultdict(dict)

    for run in runs:
        if not isinstance(run, dict) or not isinstance(run.get("arm"), str):
            raise ValueError("every run must identify its arm")
        arm = run["arm"]
        token_usage = _validate_token_usage(run)
        metrics = run.get("metrics")
        trajectory_envelope = run.get("trajectory")
        if not isinstance(metrics, dict) or not isinstance(trajectory_envelope, dict):
            raise ValueError("every run must contain metrics and trajectory")
        trajectory = trajectory_envelope.get("summary")
        if not isinstance(trajectory, dict):
            raise ValueError("every run trajectory must contain its validated summary")
        normalized_metrics = {
            field: _number(metrics[field], f"metrics.{field}")
            for field in METRIC_FIELDS if field in metrics and metrics[field] is not None
        }
        for required in ("wallMs", "cpuUserMs", "cpuSystemMs", "peakRssBytes", "contextCharacters"):
            if required not in normalized_metrics:
                raise ValueError(f"metrics.{required} is required")
        normalized_trajectory = {
            field: _integer(trajectory.get(field), f"trajectory.{field}")
            for field in TRAJECTORY_FIELDS
        }
        anchor = run.get("anchorResult")
        if not isinstance(anchor, dict) or not isinstance(anchor.get("passed"), bool):
            raise ValueError("every run must contain a deterministic anchorResult")
        grouped[arm].append({
            "run": run,
            "tokens": token_usage,
            "metrics": normalized_metrics,
            "trajectory": normalized_trajectory,
        })
        key_base = (run.get("caseId"), run.get("pass"), run.get("cacheCohort"))
        for field, value in normalized_metrics.items():
            pair_index[(*key_base, "performance", field)][arm] = value
        for field, value in token_usage.items():
            pair_index[(*key_base, "tokens", field)][arm] = float(value)

    arms: dict[str, object] = {}
    for arm, items in sorted(grouped.items()):
        deterministic = [bool(item["run"]["anchorResult"]["passed"]) for item in items]
        scores = judge_scores.get(arm, {})
        confirmations = confirmation_scores.get(arm, {})
        cohorts: dict[str, object] = {}
        for cohort in sorted({str(item["run"].get("cacheCohort")) for item in items}):
            cohort_items = [item for item in items if str(item["run"].get("cacheCohort")) == cohort]
            cohorts[cohort] = {
                field: _clean_numbers(_summary(item["metrics"][field] for item in cohort_items if field in item["metrics"]))
                for field in METRIC_FIELDS
            }
        arms[arm] = {
            "runs": len(items),
            "quality": {
                "deterministicPassRate": sum(deterministic) / len(deterministic),
                "judge": {
                    f"{field}Mean": statistics.fmean(scores[field]) if scores.get(field) else None
                    for field in SCORE_FIELDS
                },
                "confirmationJudge": {
                    f"{field}Mean": statistics.fmean(confirmations[field]) if confirmations.get(field) else None
                    for field in SCORE_FIELDS
                },
            },
            "performance": {
                **{
                    field: _clean_numbers(_summary(item["metrics"][field] for item in items if field in item["metrics"]))
                    for field in METRIC_FIELDS
                },
                "cacheCohorts": cohorts,
            },
            "tokens": {
                field: _clean_numbers(_summary(float(item["tokens"][field]) for item in items))
                for field in TOKEN_FIELDS
            },
            "trajectory": {
                **{
                    field: sum(item["trajectory"][field] for item in items)
                    for field in TRAJECTORY_FIELDS
                },
                "schemaInvalidRate": (
                    sum(item["trajectory"]["schemaInvalidCalls"] for item in items)
                    / sum(item["trajectory"]["totalCalls"] for item in items)
                    if sum(item["trajectory"]["totalCalls"] for item in items) else 0.0
                ),
                "unproductiveEmptyRate": (
                    sum(item["trajectory"]["unproductiveEmptyCalls"] for item in items)
                    / sum(item["trajectory"]["totalCalls"] for item in items)
                    if sum(item["trajectory"]["totalCalls"] for item in items) else 0.0
                ),
                "schemaRepairRate": (
                    sum(item["trajectory"]["repairedSchemaCalls"] for item in items)
                    / sum(item["trajectory"]["schemaInvalidCalls"] for item in items)
                    if sum(item["trajectory"]["schemaInvalidCalls"] for item in items) else 0.0
                ),
            },
        }

    pairwise: dict[str, object] = {}
    if "octocode" in arms:
        for baseline in sorted(set(arms) - {"octocode"}):
            ratios: dict[str, dict[str, list[float]]] = {
                "performance": defaultdict(list),
                "tokens": defaultdict(list),
            }
            for key, values in pair_index.items():
                if {"octocode", baseline}.issubset(values):
                    ratio = values[baseline] / values["octocode"] if values["octocode"] else 0
                    ratios[key[-2]][key[-1]].append(ratio)
            performance_ratios = {
                field: _geometric_ratio_summary(values)
                for field, values in sorted(ratios["performance"].items())
            }
            token_ratios = {
                field: _geometric_ratio_summary(values)
                for field, values in sorted(ratios["tokens"].items())
            }
            octo_trajectory = arms["octocode"]["trajectory"]
            baseline_trajectory = arms[baseline]["trajectory"]
            pairwise[f"octocode-vs-{baseline}"] = {
                "performanceRatios": performance_ratios,
                "tokenRatios": token_ratios,
                "wallRatio": performance_ratios.get("wallMs", _geometric_ratio_summary([])),
                "tokenRatio": token_ratios.get("totalTokens", _geometric_ratio_summary([])),
                "deterministicPassRateDelta": (
                    arms["octocode"]["quality"]["deterministicPassRate"]
                    - arms[baseline]["quality"]["deterministicPassRate"]
                ),
                "trajectoryRateDeltas": {
                    field: octo_trajectory[field] - baseline_trajectory[field]
                    for field in ("schemaInvalidRate", "unproductiveEmptyRate", "schemaRepairRate")
                },
            }

    overhead: dict[str, object] = {}
    overhead_by_role: dict[str, list[dict[str, int]]] = defaultdict(list)
    for receipt in manifest.get("roleReceipts", []):
        if not isinstance(receipt, dict):
            raise ValueError("roleReceipts entries must be objects")
        role = receipt.get("role")
        if role not in {"judge", "confirmationJudge"}:
            continue
        overhead_by_role[str(role)].append(_validate_token_usage(receipt))
    for role, usages in sorted(overhead_by_role.items()):
        overhead[role] = {
            field: _clean_numbers(_summary(float(usage[field]) for usage in usages))
            for field in TOKEN_FIELDS
        }

    return {
        "version": 3,
        "meta": {
            "tokenMeasurement": "provider-reported",
            "contextCharactersAreTokens": False,
            "pairing": "case/pass/cache-cohort",
        },
        "arms": arms,
        "pairwise": pairwise,
        "campaignOverheadTokens": overhead,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--contracts", type=Path, required=True)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        manifest = json.loads(args.campaign.read_text(encoding="utf-8"))
        bundle, contract_errors = load_and_validate_contracts(args.contracts)
        suite_errors, suite_summary = validate_suite_contract(args.suite)
        public = json.loads((args.suite / "public-cases.json").read_text(encoding="utf-8"))
        private = json.loads((args.suite / "private-manifest.json").read_text(encoding="utf-8"))
        campaign_errors = validate_campaign_manifest(
            manifest,
            suite_cases=[*public.get("cases", []), *private.get("cases", [])],
            arm_contracts=bundle.get("arms"),
            role_contract=bundle.get("roles"),
            campaign_ready=bool(suite_summary.get("campaignReady")),
        )
        errors = [*contract_errors, *suite_errors, *campaign_errors]
        if errors:
            raise ValueError("campaign is not valid: " + "; ".join(errors))
        report = build_comparison(manifest)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        print(json.dumps({"valid": False, "errors": [str(exc)]}))
        return 1
    payload = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(payload, encoding="utf-8")
    else:
        print(payload, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
