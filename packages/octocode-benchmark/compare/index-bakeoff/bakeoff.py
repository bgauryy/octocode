#!/usr/bin/env python3
"""Correctness-first linear-index versus live-Octocode bake-off."""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


HERE = Path(__file__).resolve().parent
WORKSPACE = HERE.parents[3]
ENGINE = WORKSPACE / "packages/octocode-engine/index.js"
INSTRUMENTER = HERE.parent / "bin/instrument_command.py"
ARM = HERE / "arm.mjs"
REQUIRED_COMMITS = {
    "langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "nextjs": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}


def load_contract(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schemaVersion") != 1:
        raise ValueError("cases schemaVersion must be 1")
    repositories = value.get("repositories")
    cases = value.get("cases")
    if not isinstance(repositories, dict) or not isinstance(cases, list):
        raise ValueError("cases must define repositories and cases")
    for name, commit in REQUIRED_COMMITS.items():
        record = repositories.get(name)
        if not isinstance(record, dict) or record.get("commit") != commit:
            raise ValueError(f"{name} must be pinned to {commit}")
        if not isinstance(record.get("path"), str):
            raise ValueError(f"{name} path is required")
    identifiers: set[str] = set()
    coverage: dict[str, set[str]] = {name: set() for name in REQUIRED_COMMITS}
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("every case must be an object")
        case_id = case.get("id")
        repository = case.get("repository")
        kind = case.get("kind")
        if not isinstance(case_id, str) or case_id in identifiers:
            raise ValueError("case ids must be unique strings")
        if repository not in REQUIRED_COMMITS or kind not in {"content", "path", "symbol"}:
            raise ValueError(f"invalid case: {case_id}")
        if not isinstance(case.get("query"), str) or not case["query"]:
            raise ValueError(f"case query is required: {case_id}")
        identifiers.add(case_id)
        coverage[repository].add(kind)
    for repository, kinds in coverage.items():
        if kinds != {"content", "path", "symbol"}:
            raise ValueError(f"{repository} must cover content, path, and symbol")
    return value


def validate_trial_budget(trials: int, *, normative: bool) -> None:
    if trials < 1:
        raise ValueError("trials must be positive")
    if normative and trials < 30:
        raise ValueError("normative campaigns require at least 30 paired trials")


def _median(values: list[float]) -> float | None:
    return round(statistics.median(values), 6) if values else None


def summarize(
    records: list[dict[str, Any]], *, normative: bool, requested_trials: int
) -> dict[str, Any]:
    validate_trial_budget(requested_trials, normative=normative)
    queries = [record for record in records if not str(record.get("caseId", "")).startswith("build:")]
    grouped: dict[str, dict[int, dict[str, dict[str, Any]]]] = {}
    for record in queries:
        grouped.setdefault(str(record["caseId"]), {}).setdefault(int(record["trial"]), {})[
            str(record["arm"])
        ] = record

    parity: list[str] = []
    unsupported: list[str] = []
    mismatched: list[str] = []
    incomplete: list[str] = []
    performance: dict[str, Any] = {}
    for case_id, trials in sorted(grouped.items()):
        pairs = [
            pair
            for _, pair in sorted(trials.items())
            if "linear-index" in pair and "live-scan" in pair
        ]
        if len(pairs) != requested_trials:
            incomplete.append(case_id)
            continue
        statuses = {
            str(pair[arm]["result"].get("status"))
            for pair in pairs
            for arm in ("linear-index", "live-scan")
        }
        if "unsupported" in statuses:
            unsupported.append(case_id)
            continue
        if statuses != {"ok"}:
            incomplete.append(case_id)
            continue
        equal = all(
            sorted(pair["linear-index"]["result"].get("items", []))
            == sorted(pair["live-scan"]["result"].get("items", []))
            for pair in pairs
        )
        if not equal:
            mismatched.append(case_id)
            continue
        parity.append(case_id)
        index_wall = [float(pair["linear-index"]["measurement"]["wall_time_ms"]) for pair in pairs]
        live_wall = [float(pair["live-scan"]["measurement"]["wall_time_ms"]) for pair in pairs]
        ratios = [index / live for index, live in zip(index_wall, live_wall, strict=True) if live > 0]
        index_rss = [
            int(pair["linear-index"]["measurement"]["memory"]["peak_process_tree_rss_bytes"])
            for pair in pairs
        ]
        live_rss = [
            int(pair["live-scan"]["measurement"]["memory"]["peak_process_tree_rss_bytes"])
            for pair in pairs
        ]
        performance[case_id] = {
            "pairedTrials": len(pairs),
            "medianIndexWallMs": _median(index_wall),
            "medianLiveWallMs": _median(live_wall),
            "medianIndexToLiveWallRatio": _median(ratios),
            "medianIndexPeakRssBytes": _median([float(value) for value in index_rss]),
            "medianLivePeakRssBytes": _median([float(value) for value in live_rss]),
        }

    builds = [record for record in records if str(record.get("caseId", "")).startswith("build:")]
    build_cost: dict[str, Any] = {}
    for record in builds:
        repository = str(record["caseId"]).split(":", 1)[1]
        build_cost.setdefault(repository, []).append(record)
    build_summary = {
        repository: {
            "trials": len(items),
            "medianWallMs": _median([float(item["measurement"]["wall_time_ms"]) for item in items]),
            "medianPeakRssBytes": _median(
                [float(item["measurement"]["memory"]["peak_process_tree_rss_bytes"]) for item in items]
            ),
            "medianDiskBytes": _median([float(item.get("diskBytes", 0)) for item in items]),
            "medianIndexedSourceBytes": _median(
                [float(item["result"].get("metadata", {}).get("indexedSourceBytes", 0)) for item in items]
            ),
        }
        for repository, items in sorted(build_cost.items())
    }

    if unsupported or mismatched or incomplete:
        verdict = "DISCARD_AS_S9_WINNER"
    elif not normative:
        verdict = "CONTINUE_DIAGNOSTIC_ONLY"
    else:
        ratios = [item["medianIndexToLiveWallRatio"] for item in performance.values()]
        build_guard = all(
            item["medianPeakRssBytes"] <= 2 * 1024 * 1024 * 1024
            and item["medianDiskBytes"] <= 1.5 * item["medianIndexedSourceBytes"]
            for item in build_summary.values()
            if item["medianIndexedSourceBytes"]
        )
        verdict = (
            "KEEP_LINEAR_FOUNDATION"
            if ratios and all(ratio <= 1 / 3 for ratio in ratios) and build_guard
            else "DISCARD_AS_S9_WINNER"
        )

    return {
        "schemaVersion": 1,
        "normative": normative,
        "requestedPairedTrials": requested_trials,
        "correctness": {
            "parityCases": parity,
            "unsupportedCases": unsupported,
            "mismatchedCases": mismatched,
            "incompleteCases": incomplete,
        },
        "performance": performance,
        "buildCost": build_summary,
        "verdict": verdict,
        "claimBoundary": (
            "Normative only with isolated Linux cgroup-v2 process-tree sensors and at least 30 paired trials."
            if normative
            else "Diagnostic only; do not publish performance or superiority claims from this run."
        ),
    }


def write_report(
    destination: Path,
    records: list[dict[str, Any]],
    *,
    normative: bool,
    requested_trials: int,
) -> dict[str, Any]:
    report = summarize(records, normative=normative, requested_trials=requested_trials)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def _git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout.strip()


def preflight(contract: dict[str, Any]) -> dict[str, dict[str, str]]:
    if not ENGINE.is_file():
        raise RuntimeError(f"workspace engine is not built: {ENGINE}")
    receipt: dict[str, dict[str, str]] = {}
    for name, expected in REQUIRED_COMMITS.items():
        root = Path(contract["repositories"][name]["path"]).resolve(strict=True)
        actual = _git(root, "rev-parse", "HEAD")
        if actual != expected:
            raise RuntimeError(f"{name} HEAD mismatch: expected {expected}, got {actual}")
        dirty = [
            line
            for line in _git(root, "status", "--porcelain=v1", "--untracked-files=all").splitlines()
            if line and not line.endswith(" .octocode-clone-meta.json")
        ]
        if dirty:
            raise RuntimeError(f"{name} checkout is dirty: {dirty[:5]}")
        receipt[name] = {
            "root": str(root),
            "commit": actual,
            "tree": _git(root, "rev-parse", "HEAD^{tree}"),
        }
    return receipt


def _directory_bytes(root: Path) -> int:
    return sum(path.stat().st_size for path in root.rglob("*") if path.is_file())


def _last_json_line(text: str) -> dict[str, Any]:
    for line in reversed(text.splitlines()):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise RuntimeError("measured arm produced no JSON object")


def measure(
    argv: list[str],
    *,
    label: str,
    logical_call_id: str,
    output: Path,
    cgroup_parent: Path | None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    log = output / "measurements.jsonl"
    artifacts = output / "artifacts"
    command = [
        sys.executable,
        str(INSTRUMENTER),
        "--log",
        str(log),
        "--artifact-dir",
        str(artifacts),
        "--label",
        label,
        "--cache-cohort",
        "warm-index" if "query" in label else "cold",
        "--logical-call-id",
        logical_call_id,
        "--timeout-seconds",
        "900",
    ]
    if cgroup_parent is not None:
        command.extend(["--cgroup-parent", str(cgroup_parent), "--strict-v3"])
    command.extend(["--", *argv])
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    if completed.returncode:
        raise RuntimeError(
            f"measurement failed ({completed.returncode}): {(completed.stderr + completed.stdout)[-2000:]}"
        )
    measurement = json.loads(log.read_text(encoding="utf-8").splitlines()[-1])
    return _last_json_line(completed.stdout), measurement


def run_campaign(args: argparse.Namespace) -> dict[str, Any]:
    validate_trial_budget(args.trials, normative=args.normative)
    if args.normative and (not sys.platform.startswith("linux") or args.cgroup_parent is None):
        raise ValueError("normative runs require Linux and --cgroup-parent")
    contract = load_contract(args.cases)
    repositories = preflight(contract)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="work-", dir=output))
    records: list[dict[str, Any]] = []
    stores: dict[str, dict[str, Any]] = {}
    try:
        for repository, receipt in repositories.items():
            for trial in range(1, args.build_trials + 1):
                home = work / f"{repository}-build-{trial}"
                store = {
                    "home": str(home),
                    "rootId": repository,
                    "rootPath": receipt["root"],
                    "sourceCommit": receipt["commit"],
                    "sourceTree": receipt["tree"],
                    "indexSchemaVersion": 1,
                    "parserSchemaVersion": 1,
                    "toolVersion": "index-bakeoff-v1",
                    "maxGenerations": 2,
                    "maxBytes": 4 * 1024 * 1024 * 1024,
                }
                request = {"engine": str(ENGINE), "store": store}
                result, measured = measure(
                    ["node", str(ARM), "build", json.dumps(request, separators=(",", ":"))],
                    label=f"build:{repository}:{trial}",
                    logical_call_id=f"build:{repository}:{trial}",
                    output=output,
                    cgroup_parent=args.cgroup_parent,
                )
                records.append(
                    {
                        "caseId": f"build:{repository}",
                        "arm": "linear-index-build",
                        "trial": trial,
                        "result": result,
                        "measurement": measured,
                        "diskBytes": _directory_bytes(home),
                    }
                )
                if trial == 1:
                    stores[repository] = store

        for case in contract["cases"]:
            repository = str(case["repository"])
            root = repositories[repository]["root"]
            request = {
                "engine": str(ENGINE),
                "store": stores[repository],
                "root": root,
                "case": case,
            }
            for trial in range(1, args.trials + 1):
                arms = ["linear-index", "live-scan"]
                if trial % 2 == 0:
                    arms.reverse()
                for arm in arms:
                    operation = "index-query" if arm == "linear-index" else "live-query"
                    result, measured = measure(
                        ["node", str(ARM), operation, json.dumps(request, separators=(",", ":"))],
                        label=f"query:{case['id']}:{arm}:{trial}",
                        logical_call_id=f"{case['id']}:{trial}",
                        output=output,
                        cgroup_parent=args.cgroup_parent,
                    )
                    records.append(
                        {
                            "caseId": case["id"],
                            "arm": arm,
                            "trial": trial,
                            "result": result,
                            "measurement": measured,
                        }
                    )
        (output / "records.json").write_text(
            json.dumps(records, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        report = write_report(
            output / "report.json",
            records,
            normative=args.normative,
            requested_trials=args.trials,
        )
        receipt = {
            "platform": platform.platform(),
            "normativeRequested": args.normative,
            "pairedTrials": args.trials,
            "buildTrials": args.build_trials,
            "repositories": repositories,
            "sourcegraphCompared": False,
        }
        (output / "receipt.json").write_text(
            json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        return report
    finally:
        if not args.keep_index:
            shutil.rmtree(work, ignore_errors=True)


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--cases", type=Path, default=HERE / "cases.json")
    value.add_argument("--output", type=Path, required=True)
    value.add_argument("--trials", type=int, default=30)
    value.add_argument("--build-trials", type=int, default=3)
    value.add_argument("--normative", action="store_true")
    value.add_argument("--cgroup-parent", type=Path)
    value.add_argument("--keep-index", action="store_true")
    return value


def main() -> int:
    args = parser().parse_args()
    try:
        report = run_campaign(args)
    except (OSError, RuntimeError, ValueError) as error:
        print(f"index bake-off failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
