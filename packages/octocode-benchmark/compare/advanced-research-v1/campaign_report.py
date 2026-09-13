#!/usr/bin/env python3
"""Verify sealed advanced-research campaign receipts and summarize execution cost."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


NATIVE_TO_REPORT = {
    "input_tokens": "inputTokens", "cached_input_tokens": "cachedInputTokens",
    "output_tokens": "outputTokens", "reasoning_output_tokens": "reasoningOutputTokens",
    "cache_write_input_tokens": "cacheWriteInputTokens",
}
REQUIRED_NATIVE = {"input_tokens", "output_tokens"}


def _integer(value):
    return type(value) is int and value >= 0


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _read_json(path, errors, label):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"{label}: {error}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{label}: expected object")
        return None
    return value


def _safe_file(root, name, errors, label):
    if not isinstance(name, str) or not name or Path(name).is_absolute():
        errors.append(f"{label}: invalid artifact path")
        return None
    path = root / name
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError:
        errors.append(f"{label}: artifact escapes root")
        return None
    return path


def _verify_hashes(root, artifacts, errors, label):
    if not isinstance(artifacts, dict):
        errors.append(f"{label}: artifact manifest is not an object")
        return
    for name, expected in artifacts.items():
        path = _safe_file(root, name, errors, f"{label}:{name}")
        if path is None:
            continue
        if not isinstance(expected, str) or len(expected) != 64:
            errors.append(f"{label}:{name}: invalid artifact digest")
        elif not path.is_file() or _digest(path) != expected:
            errors.append(f"{label}:{name}: artifact_hash_mismatch")


def _events(path, errors, label):
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
        values = [json.loads(line) for line in lines if line.strip()]
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"{label}: invalid events.jsonl: {error}")
        return []
    if not all(isinstance(value, dict) for value in values):
        errors.append(f"{label}: events must be JSON objects")
        return []
    return values


def _usage(row, events, errors, label):
    before = len(errors)
    receipts = [event.get("usage", {}) for event in events if event.get("type") == "turn.completed"]
    reported_receipts = row.get("nativeUsageReceipts")
    if receipts != reported_receipts:
        errors.append(f"{label}: native_usage_receipts_mismatch")
    if len(receipts) != 1:
        errors.append(f"{label}: native_usage_receipt_count={len(receipts)}")
        return None
    native = receipts[0]
    if not isinstance(native, dict):
        errors.append(f"{label}: malformed_native_usage")
        return None
    for name in NATIVE_TO_REPORT:
        if name not in native and name not in REQUIRED_NATIVE:
            continue
        if not _integer(native.get(name)):
            errors.append(f"{label}: invalid_native_usage:{name}")
    if any(not _integer(native.get(name)) for name in REQUIRED_NATIVE):
        return None
    if native.get("cached_input_tokens", 0) > native["input_tokens"]:
        errors.append(f"{label}: invalid_cached_input_tokens")
    if native.get("reasoning_output_tokens", 0) > native["output_tokens"]:
        errors.append(f"{label}: invalid_reasoning_output_tokens")
    reported = row.get("usage")
    if not isinstance(reported, dict):
        errors.append(f"{label}: usage is not an object")
        return None
    for native_name, report_name in NATIVE_TO_REPORT.items():
        expected = native.get(native_name)
        if reported.get(report_name) != expected:
            errors.append(f"{label}: usage_mismatch:{report_name}")
    total = native["input_tokens"] + native["output_tokens"]
    if reported.get("totalTokens") != total:
        errors.append(f"{label}: usage_mismatch:totalTokens")
    expected_uncached = (native["input_tokens"] - native["cached_input_tokens"]
                         if "cached_input_tokens" in native else None)
    if reported.get("uncachedInputTokens") != expected_uncached:
        errors.append(f"{label}: usage_mismatch:uncachedInputTokens")
    if len(errors) != before:
        return None
    return {"inputTokens": native["input_tokens"], "cachedInputTokens": native.get("cached_input_tokens"),
            "outputTokens": native["output_tokens"], "reasoningOutputTokens": native.get("reasoning_output_tokens"),
            "cacheWriteInputTokens": native.get("cache_write_input_tokens"), "totalTokens": total}


def _commands(row, errors, label):
    commands = row.get("commands")
    if not isinstance(commands, list) or not all(isinstance(command, dict) for command in commands):
        errors.append(f"{label}: commands is not an object list")
        return
    if row.get("shellCalls") != len(commands):
        errors.append(f"{label}: shellCalls_mismatch")
    values = [command.get("rawOutputBytes") for command in commands]
    if not all(_integer(value) for value in values):
        errors.append(f"{label}: invalid_command_raw_bytes")
        return
    if row.get("rawToolOutputBytes") != sum(values):
        errors.append(f"{label}: rawToolOutputBytes_mismatch")


def _ratio(numerator, denominator):
    if isinstance(numerator, (int, float)) and not isinstance(numerator, bool) and numerator >= 0 and isinstance(denominator, (int, float)) and not isinstance(denominator, bool) and denominator > 0:
        return numerator / denominator
    return None


def _replay_current_protocol(report, rows, root, errors):
    import cli_input
    import pilot

    PROTOCOL = pilot.PROTOCOL
    protocol = report.get("plan", {}).get("protocol")
    result = {"attempted": False, "protocol": protocol, "supported": protocol == PROTOCOL}
    if protocol != PROTOCOL:
        return result
    preflight = report.get("preflight")
    if not isinstance(preflight, dict):
        result["reason"] = "current protocol lacks frozen preflight"
        return result
    cli, bridge_name = preflight.get("cli"), preflight.get("flagBridge")
    roots = [item.get("path") for item in preflight.get("corpora", {}).values() if isinstance(item, dict)]
    bridge = _safe_file(root, str(Path(bridge_name).resolve().relative_to(root)) if isinstance(bridge_name, str)
                        and Path(bridge_name).resolve().is_relative_to(root) else None,
                        errors, "preflight.flagBridge")
    expected_hashes = {
        "flagBridgeSha256": _digest(bridge) if bridge is not None and bridge.is_file() else None,
        "flagParserSha256": _digest(Path(cli_input.__file__)),
        "flagBridgeSourceSha256": _digest(Path(cli_input._BRIDGE_SOURCE)),
        "runnerSha256": _digest(Path(pilot.__file__)),
    }
    if (not isinstance(cli, str) or bridge is None or not bridge.is_file()
            or not all(isinstance(root_path, str) for root_path in roots)):
        result["reason"] = "current protocol lacks replay inputs"
        return result
    for name, actual in expected_hashes.items():
        if preflight.get(name) != actual:
            errors.append(f"preflight: frozen_metadata_mismatch:{name}")
    if any(preflight.get(name) != actual for name, actual in expected_hashes.items()):
        result["reason"] = "frozen replay metadata differs from current implementation"
        return result
    result["attempted"] = True
    for row, events in rows:
        label = f"{row.get('case')}:{row.get('passNumber')}:{row.get('arm')}"
        if row.get("protocol") != PROTOCOL:
            errors.append(f"{label}: trial_protocol_mismatch")
            continue
        try:
            audit = pilot.EventAudit(
                pilot.Policy(row["arm"], cli, roots, remote=bool(preflight.get("remote")), flag_bridge=bridge),
                pilot.Budgets(**row["budgets"]),
            )
            for event in events:
                audit.feed(event)
            replay = audit.finish(row.get("exitCode"), bool(row.get("timedOut")))
        except (KeyError, TypeError, ValueError) as error:
            errors.append(f"{label}: audit_replay_error:{error}")
            continue
        for key in ("eligible", "failures", "usage", "nativeUsageReceipts", "shellCalls", "rawToolOutputBytes"):
            if replay.get(key) != row.get(key):
                errors.append(f"{label}: audit_replay_mismatch:{key}")
    return result


def verify_campaign(root):
    root, errors = Path(root).resolve(), []
    report = _read_json(root / "report.json", errors, "report.json")
    manifest = _read_json(root / "manifest.json", errors, "manifest.json")
    if report is None:
        return {"campaign": str(root), "errors": errors}
    if manifest is not None:
        _verify_hashes(root, manifest, errors, "campaign")
    if report.get("candidateUnchanged") is not True:
        errors.append("candidateUnchanged is not true")
    if report.get("fatalError") is not None:
        errors.append("fatalError is present")
    plan, results = report.get("plan"), report.get("results")
    if not isinstance(plan, dict) or not isinstance(results, list):
        errors.append("report plan/results malformed")
        return {"campaign": str(root), "errors": errors}
    scheduled = plan.get("trials") if _integer(plan.get("trials")) else 0
    if scheduled == 0:
        errors.append("plan.trials is not a positive integer")
    rows, verified_rows, arms, identities = [], [], {}, set()
    for row in results:
        if not isinstance(row, dict):
            errors.append("result is not an object")
            continue
        label = f"{row.get('case')}:{row.get('passNumber')}:{row.get('arm')}"
        case, arm, pass_number = row.get("case"), row.get("arm"), row.get("passNumber")
        if not isinstance(case, str) or not isinstance(arm, str) or not _integer(pass_number) or pass_number < 1:
            errors.append(f"{label}: invalid trial identity")
            continue
        identity = (case, pass_number, arm)
        if identity in identities:
            errors.append(f"{label}: duplicate trial identity")
            continue
        identities.add(identity)
        trial = _safe_file(root, f"{case}-p{pass_number:02}-{arm}", errors, label)
        before = len(errors)
        if trial is None or not trial.is_dir():
            errors.append(f"{label}: missing trial directory")
            continue
        _verify_hashes(trial, row.get("artifacts"), errors, label)
        events = _events(trial / "events.jsonl", errors, label)
        usage = _usage(row, events, errors, label)
        _commands(row, errors, label)
        trusted = len(errors) == before
        entry = {"case": case, "pass": pass_number, "arm": arm, "reportedEligible": row.get("eligible") is True,
                 "verified": trusted, "eligible": trusted and row.get("eligible") is True, "usage": usage,
                 "wallSeconds": row.get("wallSeconds"), "shellCalls": row.get("shellCalls")}
        rows.append((row, events))
        verified_rows.append(entry)
        stats = arms.setdefault(arm, {"trials": 0, "eligibleTrials": 0, "knownUsageTrials": 0,
                                     "unknownUsageTrials": 0, "inputTokens": 0, "cachedInputTokens": 0,
                                     "outputTokens": 0, "wallSeconds": 0, "shellCalls": 0})
        stats["trials"] += 1
        stats["eligibleTrials"] += entry["eligible"]
        for key in ("wallSeconds", "shellCalls"):
            if _ratio(entry[key], 1) is None:
                errors.append(f"{label}: invalid {key}")
            else:
                stats[key] += entry[key]
        if usage is None or not trusted:
            stats["unknownUsageTrials"] += 1
        else:
            stats["knownUsageTrials"] += 1
            for key in ("inputTokens", "cachedInputTokens", "outputTokens"):
                if usage[key] is not None:
                    stats[key] += usage[key]
    replay = _replay_current_protocol(report, rows, root, errors)
    pairs, pair_rows = [], {}
    for entry in verified_rows:
        pair_rows.setdefault((entry["case"], entry["pass"]), []).append(entry)
    campaign_eligible = report.get("candidateUnchanged") is True and report.get("fatalError") is None
    reported_pairs = {(pair.get("case"), pair.get("pass")): pair.get("eligible")
                      for pair in report.get("pairs", []) if isinstance(pair, dict)}
    for case in plan.get("cases", []):
        for pass_number in range(1, plan.get("passes", 0) + 1):
            entries = sorted(pair_rows.get((case, pass_number), []), key=lambda entry: entry["arm"])
            eligible = campaign_eligible and len(entries) == 2 and all(entry["eligible"] for entry in entries)
            ratios = {}
            if eligible:
                left, right = entries
                ratios[f"{left['arm']}/{right['arm']}"] = {
                    "totalTokens": _ratio(left["usage"]["totalTokens"], right["usage"]["totalTokens"]),
                    "inputTokens": _ratio(left["usage"]["inputTokens"], right["usage"]["inputTokens"]),
                    "outputTokens": _ratio(left["usage"]["outputTokens"], right["usage"]["outputTokens"]),
                    "wallSeconds": _ratio(left["wallSeconds"], right["wallSeconds"]),
                    "shellCalls": _ratio(left["shellCalls"], right["shellCalls"]),
                }
            pairs.append({"case": case, "pass": pass_number, "completedArms": len(entries), "eligible": eligible,
                          "reportedEligible": reported_pairs.get((case, pass_number)), "quality": "ungraded",
                          "costRatios": ratios})
    known = [entry for entry in verified_rows if entry["verified"] and entry["usage"] is not None]
    return {"campaign": str(root), "verified": not errors, "errors": errors,
            "candidateUnchanged": report.get("candidateUnchanged"), "fatalError": report.get("fatalError"),
            "scheduledTrials": scheduled, "completedTrials": len(verified_rows),
            "eligibleTrials": sum(entry["eligible"] for entry in verified_rows),
            "usage": {"knownTrials": len(known), "unknownTrials": len(verified_rows) - len(known),
                      "inputTokens": sum(entry["usage"]["inputTokens"] for entry in known),
                      "outputTokens": sum(entry["usage"]["outputTokens"] for entry in known),
                      "totalTokens": sum(entry["usage"]["totalTokens"] for entry in known)},
            "arms": arms, "pairs": pairs, "auditReplay": replay}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    result = verify_campaign(args.campaign)
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")
    return 0 if result.get("verified") else 1


if __name__ == "__main__":
    raise SystemExit(main())
