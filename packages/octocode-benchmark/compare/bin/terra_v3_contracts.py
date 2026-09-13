#!/usr/bin/env python3
"""Frozen arm, Terra role, suite, grader, and campaign contracts for v3."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any

from terra_v3_preflight import digest_record, validate_octocode_argv
from terra_v3_trajectory import summarize_trajectory, validate_trajectory


MODEL = "gpt-5.6-terra"
PASSES = (1, 2, 3)
ARM_IDS = ("octocode", "ripgrep", "ast-grep", "lsp-pyright", "lsp-typescript", "sourcegraph")
LANES = ("lexical", "structural", "semantic", "indexed", "codemod-preview", "end-to-end")
LOCKED_REPOSITORIES = {
    "langchain-ai/langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "vercel/next.js": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
TOKEN_KEYS = {"inputTokens", "outputTokens", "totalTokens"}
OPTIONAL_TOKEN_KEYS = {"cachedInputTokens", "reasoningTokens"}
METRIC_KEYS = {
    "wallMs", "cpuUserMs", "cpuSystemMs", "peakRssBytes", "contextCharacters",
}
OPTIONAL_METRIC_KEYS = {"cgroupPeakBytes", "ioReadBytes", "ioWriteBytes"}


def _load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_and_validate_contracts(root: Path) -> tuple[dict[str, object], list[str]]:
    errors: list[str] = []
    try:
        arms_doc = _load(root / "arms.json")
        roles = _load(root / "terra-roles.json")
    except (OSError, json.JSONDecodeError) as exc:
        return {}, [str(exc)]
    arms = arms_doc.get("arms", {})
    if set(arms) != set(ARM_IDS):
        errors.append(f"arms must be exactly {', '.join(ARM_IDS)}")
    for arm_id, arm in arms.items():
        if not isinstance(arm, dict):
            errors.append(f"arm {arm_id} must be an object")
            continue
        if arm.get("transport") not in {"raw-cli", "workspace-cli", "direct-json-rpc"}:
            errors.append(f"arm {arm_id} has invalid transport")
        lanes = arm.get("lanes")
        if not isinstance(lanes, list) or not lanes or any(lane not in LANES for lane in lanes):
            errors.append(f"arm {arm_id} has invalid lanes")
        if arm_id in {"lsp-pyright", "lsp-typescript"}:
            if arm.get("entrypoint") != "resolved-language-server-receipt" or arm.get("resolvedReceiptRequired") is not True:
                errors.append(f"arm {arm_id} must require a resolved language-server receipt")
        elif not isinstance(arm.get("versionCommand"), list) or not arm.get("versionCommand"):
            errors.append(f"arm {arm_id} lacks a version command")
        if arm.get("allowsEmulation") is not False:
            errors.append(f"arm {arm_id} must forbid emulation")
    if roles.get("version") != 3 or roles.get("model") != MODEL:
        errors.append(f"all roles must freeze model {MODEL}")
    role_map = roles.get("roles")
    if not isinstance(role_map, dict) or set(role_map) != {"runner", "judge", "confirmationJudge"}:
        errors.append("roles must be runner, judge, and confirmationJudge")
    else:
        for role_id, role in role_map.items():
            if role.get("freshContext") is not True:
                errors.append(f"{role_id} must use a fresh context")
            if role.get("reasoningEffort") not in {"low", "medium", "high", "xhigh", "max", "ultra"}:
                errors.append(f"{role_id} has invalid reasoning effort")
        if role_map["judge"].get("blind") is not True or role_map["confirmationJudge"].get("orderReversed") is not True:
            errors.append("judge contracts must freeze blind and reversed-order confirmation")
    return {"arms": arms, "roles": roles, "digest": digest_record({"arms": arms_doc, "roles": roles})}, errors


def validate_arm_argv(arm: str, argv: list[str], workspace: Path) -> list[str]:
    if arm not in ARM_IDS:
        return [f"unknown arm: {arm}"]
    if not argv:
        return ["empty argv"]
    executable = Path(argv[0]).name
    errors: list[str] = []
    if arm == "octocode":
        return validate_octocode_argv(argv, workspace)
    if arm == "ripgrep":
        if executable != "rg":
            errors.append("ripgrep arm must execute the raw rg binary")
        if "--json" not in argv:
            errors.append("ripgrep arm requires --json")
    elif arm == "ast-grep":
        if executable not in {"ast-grep", "sg"}:
            errors.append("ast-grep arm must execute the raw ast-grep/sg binary")
        if not any(value == "--json" or value.startswith("--json=") for value in argv):
            errors.append("ast-grep arm requires JSON output")
    elif arm == "lsp-pyright":
        errors.append("Pyright lane requires a validated resolved language-server receipt")
    elif arm == "lsp-typescript":
        errors.append("TypeScript lane requires a validated resolved language-server receipt")
    elif arm == "sourcegraph":
        if executable != "src" or argv[1:4] != ["search", "-json", "-stream"]:
            errors.append("Sourcegraph arm must use src search -json -stream")
    if executable in {"python", "python3", "node", "bash", "sh", "npx", "npm", "yarn", "pnpm"}:
        errors.append(f"{arm} arm cannot emulate or package-run its specialist")
    return errors


def _is_nonnegative_number(value: object) -> bool:
    return not isinstance(value, bool) and isinstance(value, (int, float)) and value >= 0


def _validate_token_usage(value: object, label: str) -> list[str]:
    if not isinstance(value, dict):
        return [f"{label} lacks provider token usage"]
    errors: list[str] = []
    unexpected = set(value) - TOKEN_KEYS - OPTIONAL_TOKEN_KEYS - {"source"}
    if unexpected:
        errors.append(f"{label} token usage has unexpected fields: {sorted(unexpected)}")
    if value.get("source") != "provider":
        errors.append(f"{label} token usage source must be provider")
    for key in TOKEN_KEYS | (set(value) & OPTIONAL_TOKEN_KEYS):
        token_count = value.get(key)
        if isinstance(token_count, bool) or not isinstance(token_count, int) or token_count < 0:
            errors.append(f"{label} {key} must be a nonnegative integer")
    if TOKEN_KEYS <= set(value):
        if value["totalTokens"] != value["inputTokens"] + value["outputTokens"]:
            errors.append(f"{label} totalTokens must equal inputTokens + outputTokens")
        if value.get("cachedInputTokens", 0) > value["inputTokens"]:
            errors.append(f"{label} cachedInputTokens exceeds inputTokens")
        if value.get("reasoningTokens", 0) > value["outputTokens"]:
            errors.append(f"{label} reasoningTokens exceeds outputTokens")
    else:
        errors.append(f"{label} token usage must contain {sorted(TOKEN_KEYS)}")
    return errors


def validate_role_receipts(
    receipts: list[dict[str, object]], role_contract: dict[str, object] | None = None,
) -> list[str]:
    errors: list[str] = []
    contexts: set[str] = set()
    effort_by_group: dict[tuple[object, object, object], object] = {}
    for index, receipt in enumerate(receipts):
        if receipt.get("model") != MODEL:
            errors.append(f"receipt {index} model is not {MODEL}")
        role = receipt.get("role")
        if role not in {"runner", "judge", "confirmationJudge"}:
            errors.append(f"receipt {index} has invalid role")
        context_id = receipt.get("contextId")
        if not isinstance(context_id, str) or not context_id:
            errors.append(f"receipt {index} lacks contextId")
        elif context_id in contexts:
            errors.append(f"receipt {index} reuses contextId {context_id}")
        else:
            contexts.add(context_id)
        key = (receipt.get("caseId"), receipt.get("pass"), role)
        effort = receipt.get("reasoningEffort")
        if role_contract:
            role_specs = role_contract.get("roles", {})
            expected_effort = role_specs.get(role, {}).get("reasoningEffort") if isinstance(role_specs, dict) else None
            if receipt.get("model") != role_contract.get("model") or effort != expected_effort:
                errors.append(f"receipt {index} differs from frozen model/effort for {role}")
        if key in effort_by_group and effort_by_group[key] != effort:
            errors.append(f"reasoning effort differs across arms for {key}")
        effort_by_group[key] = effort
        errors.extend(_validate_token_usage(receipt.get("tokenUsage"), f"receipt {index}"))
    return errors


def _validate_public_oracle_receipt(
    receipt: object, public_suite: dict[str, object],
) -> tuple[list[str], dict[str, object]]:
    """Validate the complete hash chain from suite definition to oracle answers."""
    if not isinstance(receipt, dict):
        return ["public oracle receipt must be an object"], {
            "status": "missing", "ready": False, "receiptDigest": None,
            "corpusReceiptDigest": None,
        }
    errors: list[str] = []
    receipt_digest = receipt.get("receiptDigest")
    unsigned = {key: value for key, value in receipt.items() if key != "receiptDigest"}
    if not HEX64.fullmatch(str(receipt_digest or "")) or receipt_digest != digest_record(unsigned):
        errors.append("public oracle receipt digest does not bind its contents")
    if receipt.get("suiteDigest") != digest_record(public_suite):
        errors.append("public oracle receipt suite digest mismatch")
    corpus_digest = receipt.get("corpusReceiptDigest")
    if not HEX64.fullmatch(str(corpus_digest or "")):
        errors.append("public oracle receipt lacks a valid corpus receipt digest")
    observed_repositories = receipt.get("repositories")
    expected_repositories = [
        {"repo": repo, "commit": commit}
        for repo, commit in sorted(LOCKED_REPOSITORIES.items())
    ]
    if observed_repositories != expected_repositories:
        errors.append("public oracle repository identifiers/commits differ from the frozen corpus")

    normalization = receipt.get("normalizationReceipt")
    if not isinstance(normalization, dict):
        errors.append("public oracle receipt lacks normalization receipt")
    else:
        normalization_unsigned = {
            key: value for key, value in normalization.items()
            if key not in {"receiptDigest", "digest"}
        }
        expected_normalization_digest = digest_record(normalization_unsigned)
        if (
            normalization.get("receiptDigest") != expected_normalization_digest
            or normalization.get("digest") != expected_normalization_digest
            or normalization.get("version") != public_suite.get("normalizationVersion")
        ):
            errors.append("public oracle normalization receipt is invalid or mismatched")

    tool_receipts = receipt.get("toolReceipts")
    if not isinstance(tool_receipts, list):
        errors.append("public oracle toolReceipts must be a list")
    else:
        for index, tool_receipt in enumerate(tool_receipts):
            if not isinstance(tool_receipt, dict):
                errors.append(f"public oracle tool receipt {index} must be an object")
                continue
            tool_unsigned = {
                key: value for key, value in tool_receipt.items() if key != "receiptDigest"
            }
            if (
                tool_receipt.get("receiptDigest") != digest_record(tool_unsigned)
                or not HEX64.fullmatch(str(tool_receipt.get("executableDigest", "")))
                or not isinstance(tool_receipt.get("argv0"), str)
                or not Path(str(tool_receipt.get("argv0"))).is_absolute()
            ):
                errors.append(f"public oracle tool receipt {index} is invalid")

    expected_cases = public_suite.get("cases", [])
    observed_cases = receipt.get("cases")
    complete = True
    if not isinstance(expected_cases, list) or not isinstance(observed_cases, list):
        errors.append("public oracle receipt case set mismatch")
        complete = False
    else:
        expected_ids = [case.get("id") for case in expected_cases if isinstance(case, dict)]
        observed_ids = [case.get("caseId") for case in observed_cases if isinstance(case, dict)]
        if observed_ids != expected_ids or len(observed_cases) != len(expected_cases):
            errors.append("public oracle receipt case set/order differs from public suite")
            complete = False
        by_id = {
            case.get("id"): case for case in expected_cases if isinstance(case, dict)
        }
        for index, item in enumerate(observed_cases):
            if not isinstance(item, dict):
                errors.append(f"public oracle case receipt {index} must be an object")
                complete = False
                continue
            case_id = item.get("caseId")
            expected = by_id.get(case_id)
            status = item.get("status")
            if status != "materialized":
                complete = False
                if status != "gap" or not isinstance(item.get("gap"), dict):
                    errors.append(f"public oracle case {case_id} has invalid status")
                continue
            if not isinstance(expected, dict):
                continue
            anchor = expected.get("anchor", {})
            case_unsigned = {key: value for key, value in item.items() if key != "receiptDigest"}
            answer = item.get("answer")
            result_count = (
                len(answer) if isinstance(answer, list)
                else len(answer.get("matches", [])) if isinstance(answer, dict)
                else -1
            )
            if item.get("receiptDigest") != digest_record(case_unsigned):
                errors.append(f"public oracle case {case_id} receipt digest mismatch")
            if item.get("answerDigest") != digest_record(answer):
                errors.append(f"public oracle case {case_id} answer digest mismatch")
            if (
                not isinstance(anchor, dict)
                or item.get("kind") != anchor.get("kind")
                or item.get("oracle") != anchor.get("oracle")
            ):
                errors.append(f"public oracle case {case_id} contract mismatch")
            if result_count <= 0 or item.get("resultCount") != result_count:
                errors.append(f"public oracle case {case_id} has invalid result count")
    claimed_status = receipt.get("status")
    if claimed_status not in {"complete", "incomplete"}:
        errors.append("public oracle receipt has invalid status")
        complete = False
    if (claimed_status == "complete") != complete:
        errors.append("public oracle receipt status does not match case materialization")
    ready = complete and not errors
    return errors, {
        "status": claimed_status, "ready": ready, "receiptDigest": receipt_digest,
        "corpusReceiptDigest": corpus_digest,
    }


def validate_suite_contract(root: Path) -> tuple[list[str], dict[str, object]]:
    errors: list[str] = []
    try:
        public = _load(root / "public-cases.json")
        private = _load(root / "private-manifest.json")
        public_oracles = _load(root / "public-oracles.json")
    except (OSError, json.JSONDecodeError) as exc:
        return [str(exc)], {"publicCount": 0, "privateCount": 0}
    public_cases = public.get("cases", [])
    private_cases = private.get("cases", [])
    if len(public_cases) != 20:
        errors.append("public suite must contain exactly 20 cases")
    if len(private_cases) != 20:
        errors.append("private manifest must contain exactly 20 cases")
    ids: set[str] = set()
    for split, cases in (("public", public_cases), ("private", private_cases)):
        for index, case in enumerate(cases):
            case_id = case.get("id") if isinstance(case, dict) else None
            if not isinstance(case_id, str) or case_id in ids:
                errors.append(f"{split} case {index} has missing/duplicate id")
            else:
                ids.add(case_id)
            if case.get("lane") not in LANES:
                errors.append(f"{split} case {index} has invalid lane")
            repos = case.get("repositories")
            if not isinstance(repos, list) or not repos or any(repo not in {"langchain-ai/langchain", "vercel/next.js"} for repo in repos):
                errors.append(f"{split} case {index} has invalid repositories")
            if split == "public":
                if not isinstance(case.get("prompt"), str) or not case.get("prompt", "").strip():
                    errors.append(f"public case {index} lacks prompt")
                anchor = case.get("anchor")
                if not isinstance(anchor, dict) or anchor.get("kind") not in {"exact-result-set", "fact-set", "patch-hash"}:
                    errors.append(f"public case {index} lacks deterministic anchor")
            else:
                if "prompt" in case or "answer" in case or "anchor" in case:
                    errors.append(f"private manifest leaks case {index} content")
                if not HEX64.fullmatch(str(case.get("envelopeDigest", ""))):
                    errors.append(f"private case {index} lacks sealed envelope digest")
    expected_lanes = set(LANES)
    for split, cases in (("public", public_cases), ("private", private_cases)):
        actual_lanes = {case.get("lane") for case in cases if isinstance(case, dict)}
        if actual_lanes != expected_lanes:
            errors.append(f"{split} suite must cover every taxonomy lane")
    oracle_errors, oracle_summary = _validate_public_oracle_receipt(public_oracles, public)
    errors.extend(oracle_errors)
    private_ready = private.get("status") == "sealed" and private.get("materializationRequired") is False
    summary = {
        "publicCount": len(public_cases), "privateCount": len(private_cases),
        "publicDigest": digest_record(public), "privateManifestDigest": digest_record(private),
        "privateStatus": private.get("status"),
        "publicOracleStatus": oracle_summary["status"],
        "publicOracleReceiptDigest": oracle_summary["receiptDigest"],
        "publicOracleCorpusDigest": oracle_summary["corpusReceiptDigest"],
        "publicOraclesReady": oracle_summary["ready"],
        "campaignReady": private_ready and oracle_summary["ready"],
    }
    return errors, summary


def grade_deterministic(expected: object, actual: object, kind: str) -> dict[str, object]:
    if kind in {"exact-result-set", "fact-set"}:
        normalize = lambda value: sorted(json.dumps(item, sort_keys=True, separators=(",", ":")) for item in value)
        passed = isinstance(expected, list) and isinstance(actual, list) and normalize(expected) == normalize(actual)
    elif kind == "patch-hash":
        actual_digest = hashlib.sha256(actual.encode("utf-8")).hexdigest() if isinstance(actual, str) else ""
        passed = expected == actual_digest
    else:
        raise ValueError(f"unsupported deterministic grader: {kind}")
    return {"passed": passed, "kind": kind, "expectedDigest": digest_record(expected), "actualDigest": digest_record(actual)}


def validate_sourcegraph_receipt(receipt: dict[str, object], corpus_lock: dict[str, object]) -> list[str]:
    errors: list[str] = []
    if not HEX64.fullmatch(str(receipt.get("instanceDigest", ""))):
        errors.append("Sourcegraph instance digest is missing")
    expected = {
        item.get("repo"): item.get("commit")
        for item in corpus_lock.get("repositories", []) if isinstance(item, dict)
    }
    observed_items = receipt.get("repositories", [])
    observed = {
        item.get("repo"): item
        for item in observed_items if isinstance(item, dict)
    } if isinstance(observed_items, list) else {}
    if set(observed) != set(expected):
        errors.append("Sourcegraph repository set differs from corpus lock")
    for repo, commit in expected.items():
        item = observed.get(repo, {})
        if item.get("indexedCommit") != commit:
            errors.append(f"Sourcegraph indexed commit mismatch for {repo}")
        if item.get("indexConverged") is not True:
            errors.append(f"Sourcegraph index has not converged for {repo}")
    return errors


def _validate_tool_receipt(value: object, arm: object, label: str) -> list[str]:
    if not isinstance(value, dict):
        return [f"{label} lacks tool receipt"]
    errors: list[str] = []
    if value.get("arm") != arm:
        errors.append(f"{label} tool receipt arm mismatch")
    if arm in {"lsp-pyright", "lsp-typescript"}:
        for key in ("executableDigest", "packageDigest", "configFingerprint", "capabilitiesDigest", "receiptDigest"):
            if not HEX64.fullmatch(str(value.get(key, ""))):
                errors.append(f"{label} language-server receipt has invalid {key}")
        command = value.get("resolvedCommand")
        if not isinstance(command, list) or not command or not Path(str(command[0])).is_absolute():
            errors.append(f"{label} language-server receipt lacks absolute resolvedCommand")
        if not isinstance(value.get("workspaceRoot"), str) or not Path(str(value["workspaceRoot"])).is_absolute():
            errors.append(f"{label} language-server receipt lacks workspaceRoot")
        readiness = value.get("readiness")
        if not isinstance(readiness, dict) or any(readiness.get(key) is not True for key in ("initializeSucceeded", "probeSucceeded", "shutdownSucceeded")):
            errors.append(f"{label} language-server receipt is not ready")
        unsigned = {key: item for key, item in value.items() if key != "receiptDigest"}
        if value.get("receiptDigest") != digest_record(unsigned):
            errors.append(f"{label} language-server receipt digest does not bind its contents")
        return errors
    for key in ("executableDigest", "receiptDigest"):
        if not HEX64.fullmatch(str(value.get(key, ""))):
            errors.append(f"{label} tool receipt has invalid {key}")
    if not isinstance(value.get("argv0"), str) or not Path(str(value["argv0"])).is_absolute():
        errors.append(f"{label} tool receipt argv0 must be absolute")
    if not isinstance(value.get("versionCommand"), list) or not value.get("versionCommand"):
        errors.append(f"{label} tool receipt lacks versionCommand")
    if not isinstance(value.get("versionOutput"), str) or not value.get("versionOutput", "").strip():
        errors.append(f"{label} tool receipt lacks versionOutput")
    unsigned = {key: item for key, item in value.items() if key != "receiptDigest"}
    if value.get("receiptDigest") != digest_record(unsigned):
        errors.append(f"{label} tool receipt digest does not bind its contents")
    return errors


def _validate_metrics(value: object, label: str) -> list[str]:
    if not isinstance(value, dict):
        return [f"{label} lacks metrics"]
    errors: list[str] = []
    for key in METRIC_KEYS:
        if key not in value or not _is_nonnegative_number(value.get(key)):
            errors.append(f"{label} metric {key} must be a nonnegative number")
    for key in set(value) & OPTIONAL_METRIC_KEYS:
        if not _is_nonnegative_number(value.get(key)):
            errors.append(f"{label} metric {key} must be a nonnegative number")
    return errors


def _validate_run_trajectory(value: object, label: str) -> list[str]:
    if not isinstance(value, dict):
        return [f"{label} lacks trajectory records and summary"]
    records = value.get("records")
    summary = value.get("summary")
    if not isinstance(records, list) or not records:
        return [f"{label} trajectory has no records"]
    if not all(isinstance(record, dict) for record in records):
        return [f"{label} trajectory contains a non-object record"]
    errors = [f"{label} trajectory: {error}" for error in validate_trajectory(records)]
    if not isinstance(summary, dict) or summary != summarize_trajectory(records):
        errors.append(f"{label} trajectory summary does not match records")
    return errors


def _validate_anchor_result(value: object, label: str) -> list[str]:
    if not isinstance(value, dict) or not isinstance(value.get("passed"), bool):
        return [f"{label} lacks deterministic anchor result"]
    errors: list[str] = []
    if value.get("kind") not in {"exact-result-set", "fact-set", "patch-hash"}:
        errors.append(f"{label} has invalid deterministic anchor kind")
    for key in ("expectedDigest", "actualDigest"):
        if not HEX64.fullmatch(str(value.get(key, ""))):
            errors.append(f"{label} has invalid anchor {key}")
    return errors


def _validate_quality_record(record: object, index: int, confirmation: bool) -> list[str]:
    label = f"{'confirmation judge' if confirmation else 'judge'} {index}"
    if not isinstance(record, dict):
        return [f"{label} must be an object"]
    errors: list[str] = []
    if "labelToArm" in record:
        errors.append(f"{label} reveals labelToArm before post-run unblinding")
    if not isinstance(record.get("judgeId"), str) or not record.get("judgeId"):
        errors.append(f"{label} lacks judgeId")
    if record.get("orderHidden") is not True:
        errors.append(f"{label} was not blinded")
    if record.get("reasonedBeforeScores") is not True:
        errors.append(f"{label} scored before reasoning")
    if confirmation and record.get("orderReversed") is not True:
        errors.append(f"{label} did not reverse answer order")
    labels = record.get("labels")
    if labels != ["X", "Y"]:
        errors.append(f"{label} must use anonymous labels X/Y")
    scores = record.get("scores")
    bounds = {"correctness": (0, 10), "depth": (1, 5), "workflow": (1, 5)}
    if not isinstance(scores, dict) or set(scores) != {"X", "Y"}:
        errors.append(f"{label} scores must be keyed by X/Y")
    else:
        for anonymous_label, dimensions in scores.items():
            if not isinstance(dimensions, dict) or set(dimensions) != set(bounds):
                errors.append(f"{label} {anonymous_label} lacks exact quality dimensions")
                continue
            for dimension, (low, high) in bounds.items():
                score = dimensions[dimension]
                if not _is_nonnegative_number(score) or not low <= score <= high:
                    errors.append(f"{label} {anonymous_label}.{dimension} is outside {low}..{high}")
    errors.extend(_validate_anchor_result(record.get("anchorResult"), label))
    return errors


def validate_campaign_manifest(
    manifest: dict[str, object], *, suite_cases: list[dict[str, object]] | None = None,
    arm_contracts: dict[str, object] | None = None,
    role_contract: dict[str, object] | None = None, campaign_ready: bool = False,
    public_oracle_receipt_digest: str | None = None,
    public_oracle_corpus_digest: str | None = None,
) -> list[str]:
    errors: list[str] = []
    runs = manifest.get("runs", [])
    judges = manifest.get("judges", [])
    confirmations = manifest.get("confirmationJudges", [])
    role_receipts = manifest.get("roleReceipts", [])
    blind_assignments = manifest.get("blindAssignments", [])
    if not campaign_ready:
        errors.append("public/private suites are not materialized, sealed, and campaign-ready")
    if not HEX64.fullmatch(str(public_oracle_receipt_digest or "")):
        errors.append("campaign validation lacks a valid frozen public oracle receipt digest")
    elif manifest.get("publicOracleReceiptDigest") != public_oracle_receipt_digest:
        errors.append("campaign public oracle receipt digest differs from frozen suite")
    if not HEX64.fullmatch(str(public_oracle_corpus_digest or "")):
        errors.append("campaign validation lacks a valid public-oracle corpus digest")
    if suite_cases is None or arm_contracts is None or role_contract is None:
        errors.append("campaign validation requires frozen suite, arm, and role contracts")
        return errors
    if not isinstance(runs, list) or not runs:
        errors.append("campaign has no runs")
        return errors
    expected_runs: set[tuple[object, object, int]] = set()
    expected_comparisons: set[tuple[object, object, int]] = set()
    for case in suite_cases:
        case_id, lane = case.get("id"), case.get("lane")
        repositories = set(case.get("repositories", []))
        applicable = {
            arm for arm, contract in arm_contracts.items()
            if isinstance(contract, dict) and lane in contract.get("lanes", [])
        }
        if "vercel/next.js" not in repositories:
            applicable.discard("lsp-typescript")
        if "langchain-ai/langchain" not in repositories:
            applicable.discard("lsp-pyright")
        if "octocode" not in applicable or len(applicable) < 2:
            errors.append(f"frozen case {case_id} has no Octocode-to-baseline comparison")
        for pass_number in PASSES:
            expected_runs.update((case_id, arm, pass_number) for arm in applicable)
            expected_comparisons.update((case_id, arm, pass_number) for arm in applicable if arm != "octocode")
    observed_runs: list[tuple[object, object, object]] = []
    workspace_digests = {run.get("workspaceReceiptDigest") for run in runs if isinstance(run, dict)}
    corpus_digests = {run.get("corpusLockDigest") for run in runs if isinstance(run, dict)}
    if len(workspace_digests) != 1 or any(not HEX64.fullmatch(str(value or "")) for value in workspace_digests):
        errors.append("runs have mixed/invalid workspace receipts")
    if len(corpus_digests) != 1 or any(not HEX64.fullmatch(str(value or "")) for value in corpus_digests):
        errors.append("runs have mixed/invalid corpus locks")
    elif public_oracle_corpus_digest and corpus_digests != {public_oracle_corpus_digest}:
        errors.append("run corpus lock differs from public oracle corpus receipt")
    answer_digests: dict[tuple[object, object, object], str] = {}
    for index, run in enumerate(runs):
        if not isinstance(run, dict):
            errors.append(f"run {index} must be an object")
            continue
        run_key = (run.get("caseId"), run.get("arm"), run.get("pass"))
        observed_runs.append(run_key)
        answer_digest = str(run.get("answerDigest", ""))
        if not HEX64.fullmatch(answer_digest):
            errors.append(f"run {index} has invalid answerDigest")
        else:
            answer_digests[run_key] = answer_digest
        errors.extend(_validate_tool_receipt(run.get("toolReceipt"), run.get("arm"), f"run {index}"))
        errors.extend(_validate_token_usage(run.get("tokenUsage"), f"run {index}"))
        errors.extend(_validate_metrics(run.get("metrics"), f"run {index}"))
        errors.extend(_validate_run_trajectory(run.get("trajectory"), f"run {index}"))
        errors.extend(_validate_anchor_result(run.get("anchorResult"), f"run {index}"))
    if set(observed_runs) != expected_runs or len(observed_runs) != len(expected_runs):
        errors.append("run matrix differs from frozen suite/lane/pass matrix")

    judge_collections = ((judges, False, "judge"), (confirmations, True, "confirmationJudge"))
    judge_ids: set[str] = set()
    judge_identity: dict[str, tuple[object, object, object, str]] = {}
    expected_receipts = {(case, arm, pass_number, "runner") for case, arm, pass_number in expected_runs}
    for collection, confirmation, role in judge_collections:
        if not isinstance(collection, list):
            errors.append(f"{role} records must be a list")
            continue
        observed = []
        for index, record in enumerate(collection):
            errors.extend(_validate_quality_record(record, index, confirmation))
            if isinstance(record, dict):
                key = (record.get("caseId"), record.get("arm"), record.get("pass"))
                observed.append(key)
                judge_id = record.get("judgeId")
                if isinstance(judge_id, str):
                    if judge_id in judge_ids:
                        errors.append(f"duplicate judgeId {judge_id}")
                    judge_ids.add(judge_id)
                    judge_identity[judge_id] = (*key, role)
                expected_receipts.add((*key, role))
        if set(observed) != expected_comparisons or len(observed) != len(expected_comparisons):
            errors.append(f"{role} matrix differs from frozen comparison matrix")

    if not isinstance(role_receipts, list):
        errors.append("roleReceipts must be a list")
    else:
        errors.extend(validate_role_receipts(role_receipts, role_contract))
        observed_receipts = [
            (item.get("caseId"), item.get("arm"), item.get("pass"), item.get("role"))
            for item in role_receipts if isinstance(item, dict)
        ]
        if set(observed_receipts) != expected_receipts or len(observed_receipts) != len(expected_receipts):
            errors.append("Terra role receipt matrix is incomplete or contains extras")
        for index, receipt in enumerate(role_receipts):
            if not isinstance(receipt, dict) or receipt.get("role") == "runner":
                continue
            identity = judge_identity.get(str(receipt.get("judgeId", "")))
            receipt_identity = (
                receipt.get("caseId"), receipt.get("arm"), receipt.get("pass"), receipt.get("role"),
            )
            if identity != receipt_identity:
                errors.append(f"receipt {index} is not bound to its judge record")

    if not isinstance(blind_assignments, list):
        errors.append("blindAssignments must be a list")
    else:
        observed_assignment_ids: list[object] = []
        assignment_mappings: dict[tuple[object, object, object, str], dict[str, object]] = {}
        for index, assignment in enumerate(blind_assignments):
            if not isinstance(assignment, dict):
                errors.append(f"blind assignment {index} must be an object")
                continue
            judge_id = assignment.get("judgeId")
            observed_assignment_ids.append(judge_id)
            comparison = (assignment.get("caseId"), assignment.get("arm"), assignment.get("pass"))
            mapping = assignment.get("labelToArm")
            digests = assignment.get("answerDigests")
            expected_arms = {"octocode", assignment.get("arm")}
            if comparison not in expected_comparisons or not isinstance(mapping, dict) or set(mapping) != {"X", "Y"} or set(mapping.values()) != expected_arms:
                errors.append(f"blind assignment {index} has invalid one-to-one label mapping")
            elif not isinstance(digests, dict) or set(digests) != {"X", "Y"}:
                errors.append(f"blind assignment {index} lacks X/Y answer digests")
            else:
                for anonymous_label, arm in mapping.items():
                    if digests.get(anonymous_label) != answer_digests.get((comparison[0], arm, comparison[2])):
                        errors.append(f"blind assignment {index} is not bound to frozen answer digests")
            unsigned = {key: value for key, value in assignment.items() if key != "assignmentDigest"}
            if assignment.get("assignmentDigest") != digest_record(unsigned):
                errors.append(f"blind assignment {index} has invalid assignmentDigest")
            identity = judge_identity.get(str(judge_id))
            if identity is None or identity[:3] != comparison:
                errors.append(f"blind assignment {index} is not bound to its judge record")
            elif isinstance(mapping, dict):
                assignment_mappings[identity] = mapping
        if set(observed_assignment_ids) != judge_ids or len(observed_assignment_ids) != len(judge_ids):
            errors.append("blind assignment matrix differs from judge records")
        for comparison in expected_comparisons:
            initial = assignment_mappings.get((*comparison, "judge"))
            confirmation = assignment_mappings.get((*comparison, "confirmationJudge"))
            if initial and confirmation and not (
                initial.get("X") == confirmation.get("Y")
                and initial.get("Y") == confirmation.get("X")
            ):
                errors.append(f"confirmation assignment did not reverse labels for {comparison}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--contracts", type=Path, required=True)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument("--campaign", type=Path)
    args = parser.parse_args()
    bundle, errors = load_and_validate_contracts(args.contracts)
    suite_errors, suite_summary = validate_suite_contract(args.suite)
    errors.extend(suite_errors)
    if args.campaign:
        public = _load(args.suite / "public-cases.json")
        private = _load(args.suite / "private-manifest.json")
        suite_cases = [
            case for case in [*public.get("cases", []), *private.get("cases", [])]
            if isinstance(case, dict)
        ]
        errors.extend(validate_campaign_manifest(
            _load(args.campaign),
            suite_cases=suite_cases,
            arm_contracts=bundle.get("arms", {}),
            role_contract=bundle.get("roles", {}),
            campaign_ready=bool(suite_summary.get("campaignReady")),
            public_oracle_receipt_digest=str(suite_summary.get("publicOracleReceiptDigest") or ""),
            public_oracle_corpus_digest=str(suite_summary.get("publicOracleCorpusDigest") or ""),
        ))
    result = {"valid": not errors, "errors": errors, "contractDigest": bundle.get("digest"), "suite": suite_summary}
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
