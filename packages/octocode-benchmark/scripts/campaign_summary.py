#!/usr/bin/env python3
"""Summarize one campaign into results/<campaign>/run.json (local results; campaigns/ is raw scratch).

Reads every <arm>-p<pass>-<Qn>.jsonl measured log, runs/<id>/result.json (usage, cost) and
judge/<A>~<B>/p<pass>-<Qn>.md verdicts (de-blinded with that pair's MAP.secret.txt).

    campaign_summary.py campaigns/<name> --octocode-label "Octocode v20 (local 20.1.1)"
"""
from __future__ import annotations

import argparse
import json
import re
from collections import Counter
from pathlib import Path

PKG = Path(__file__).resolve().parents[1]
LOG = re.compile(r"^(?P<arm>.+)-p(?P<pass>\d+)-(?P<q>Q\d+)\.jsonl$")


def semantic_calls(calls: list[dict]) -> dict:
    """Measure requested semantic work without mistaking replay for a fresh decision."""
    tools, failed, goal_tools, replay_tools = Counter(), Counter(), Counter(), Counter()
    char_rows = {}
    goals = replay = initial_goals = 0
    for row in calls:
        name, _, raw = row["cmd"].partition(" ")
        tools[name] += 1
        failed[name] += bool(row.get("exit_code"))
        char_rows.setdefault(name, []).append(row.get("total_chars"))
        try:
            payload = json.loads(raw)
        except (ValueError, TypeError):
            continue
        for query in payload.get("queries", []) if isinstance(payload, dict) else []:
            if not isinstance(query, dict):
                continue
            # Preserve historical record interpretation; opaque current cursors reveal no work type.
            classification = query.get("classification")
            predicates = classification.get("predicates") if isinstance(classification, dict) else None
            requested = bool(predicates or query.get("classifiers") or query.get("goals"))
            goals += requested
            replay += bool(query.get("ranking"))
            initial_goals += requested and not query.get("ranking")
            goal_tools[name] += requested
            replay_tools[name] += bool(query.get("ranking"))
    return {"tool_calls": dict(tools), "tool_failed_calls": dict(failed),
            "tool_chars": {name: sum(values) if all(v is not None for v in values) else None
                           for name, values in char_rows.items()},
            "goal_queries": goals, "initial_goal_queries": initial_goals,
            "goal_tool_queries": dict(goal_tools), "ranking_replay_queries": replay,
            "ranking_replay_tool_queries": dict(replay_tools)}


def codex_receipts(path: Path) -> dict | None:
    """Count query failures even when the enclosing MCP call completed successfully."""
    if not path.exists():
        return None
    rankings, selections, errors = [], [], Counter()
    observed = False
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        event = json.loads(line)
        item = event.get("item", {})
        if event.get("type") != "item.completed" or item.get("type") != "mcp_tool_call":
            continue
        observed = True
        for row in ((item.get("result") or {}).get("structured_content") or {}).get("results", []):
            data = row.get("data") or {}
            if data.get("errorCode") or row.get("status") == "error":
                errors[(item["tool"], data.get("errorCode", "unknown"))] += 1
            rank = data.get("ranking")
            if isinstance(rank, dict):
                rankings.append({"tool": item["tool"], **{k: rank[k] for k in
                    ("status", "reason", "scope", "rankedItems", "totalItems") if k in rank}})
            selection = data.get("selection")
            if isinstance(selection, dict):
                selections.append({"tool": item["tool"], **selection})
    return {"ranking_receipts": rankings, "selection_receipts": selections,
            "query_errors": [{"tool": tool, "code": code, "count": count}
                             for (tool, code), count in sorted(errors.items())]} if observed else None


def classification_http(path: Path) -> dict | None:
    if not path.exists():
        return None
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    started = [row for row in rows if row.get("event") == "started"]
    finished = {row["id"]: row for row in rows if row.get("event") == "finished"}
    complete = len(started) == len(finished)
    known_input = [r["inputTokens"] for r in finished.values() if r.get("inputTokens") is not None]
    known_output = [r["outputTokens"] for r in finished.values() if r.get("outputTokens") is not None]
    return {
        "attempts": len(started), "completed": len(finished),
        "failed_or_unknown": sum(not 200 <= (finished.get(r["id"], {}).get("httpStatus") or 0) < 300 for r in started),
        "known_input_tokens": sum(known_input), "known_output_tokens": sum(known_output),
        "input_tokens_complete": complete and len(known_input) == len(started),
        "output_tokens_complete": complete and len(known_output) == len(started),
        "resolved_models": dict(Counter(r["resolvedModel"] for r in finished.values() if r.get("resolvedModel"))),
        "rejected_before_upstream": sum(r.get("event") == "rejected" for r in rows),
        "scope": "All traced transport attempts, including preflight/startup probes; unknown usage is not zero",
    }


def run_row(camp: Path, log: Path, *, identity: str | None = None, run_dir: Path | None = None) -> dict:
    m = LOG.match(identity or log.name)
    recs = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines() if l.strip()]
    calls = [r for r in recs if r.get("kind") != "answer"]
    result_path = (run_dir or camp / "runs" / log.name[:-6]) / "result.json"
    res = json.loads(result_path.read_text()) if result_path.exists() else {}
    usage = res.get("usage") or {}
    status_path = result_path.with_name("status.json")
    status = json.loads(status_path.read_text()) if status_path.exists() else {}
    wall_ms, api_ms = res.get("duration_ms"), res.get("duration_api_ms")
    input_tokens = usage.get("input_tokens")
    if input_tokens is not None:
        input_tokens += usage.get("cache_creation_input_tokens", 0) or 0
        input_tokens += usage.get("cache_read_input_tokens", 0) or 0
    successful = res.get("subtype") == "success" and not res.get("is_error")
    return {
        "arm": m["arm"], "pass": int(m["pass"]), "q": m["q"],
        "chars": sum(r["total_chars"] for r in recs),
        "model_in": sum(r["model_in_chars"] for r in recs),
        "model_out": sum(r["model_out_chars"] for r in recs),
        "calls": len(calls),
        "failed": sum(1 for r in calls if r.get("exit_code")),
        "clasify_calls": sum(1 for r in calls if r["cmd"].startswith("clasify ")),
        **semantic_calls(calls),
        "input_tokens": input_tokens,
        "cached_input_tokens": usage.get("cached_input_tokens"),
        "uncached_input_tokens": input_tokens - usage["cached_input_tokens"]
            if input_tokens is not None and usage.get("cached_input_tokens") is not None else None,
        "output_tokens": usage.get("output_tokens"),
        "cost_usd": res.get("total_cost_usd"),
        "turns": res.get("num_turns"),
        # Wall-clock seconds for the whole run, split into model time (API) and the rest:
        # tool calls plus harness overhead. Same clock for every arm of a campaign.
        "secs": wall_ms / 1000 if wall_ms is not None else None,
        "setup_secs": res["setup_duration_ms"] / 1000 if res.get("setup_duration_ms") is not None else None,
        "solver_secs": res["solver_duration_ms"] / 1000 if res.get("solver_duration_ms") is not None else None,
        "model_secs": api_ms / 1000 if api_ms is not None else None,
        "tool_secs": max(0.0, (wall_ms - api_ms) / 1000) if wall_ms is not None and api_ms is not None else None,
        "ok": successful and status.get("ok", True),
        "problems": status.get("problems", []),
        "classification_usage": res.get("classification_usage"),
        "classification_http": classification_http(result_path.with_name("classification-http.jsonl")),
        "codex_receipts": codex_receipts(result_path.with_name("transcript.jsonl")),
    }


def parse_verdict(text: str) -> dict:
    blocks = re.findall(r"```json\s*(\{.*?\})\s*```", text, re.S)
    if not blocks:
        raise ValueError("Missing fenced judgment")
    value = json.loads(blocks[-1])
    for arm in ("X", "Y"):
        for field, lower, upper in (("correctness", 0, 10), ("depth", 1, 5), ("workflow", 1, 5)):
            score = value.get(arm, {}).get(field)
            if field == "workflow" and score is None:
                continue
            if type(score) is not int or not lower <= score <= upper:
                raise ValueError(f"Invalid {arm}.{field} score")
    if value.get("winner") not in ("X", "Y", "tie") or not isinstance(value.get("reason"), str) or not value["reason"].strip():
        raise ValueError("Missing winner or reason")
    return value


def judgments(camp: Path) -> list[dict]:
    out = []
    for pair_dir in sorted((camp / "judge").glob("*~*")):
        mapping = {}
        for line in (pair_dir / "MAP.secret.txt").read_text().splitlines():
            p, q, x, y = line.split()
            mapping[(int(p[1:]), q)] = {"X": x.split("=", 1)[1], "Y": y.split("=", 1)[1]}
        for verdict in sorted(pair_dir.glob("p*-Q*.md")):
            if verdict.name.endswith(".packet.md"):
                continue
            p, q = verdict.stem.split("-")
            try:
                v = parse_verdict(verdict.read_text())
            except (ValueError, AttributeError):
                continue
            labels = mapping[(int(p[1:]), q)]
            out.append({
                "pair": pair_dir.name, "pass": int(p[1:]), "q": q,
                "scores": {labels[k]: v[k] for k in "XY"},
                "winner": labels.get(v.get("winner"), "tie"),
                "reason": v.get("reason", ""),
            })
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("campaign", type=Path)
    ap.add_argument("--octocode-label", default="Octocode v20")
    ap.add_argument("--runner-model", default="claude-sonnet-5-5")
    ap.add_argument("--judge-model", default="gpt-5.5")
    ap.add_argument("--question-set", help="Corpus identity; prevents pooling unrelated Q1s")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    camp = args.campaign.resolve()
    manifest_path = camp / "campaign.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    rows = [run_row(camp, p) for p in sorted(camp.glob("*-p*-Q*.jsonl")) if LOG.match(p.name)]
    attempts = []
    for log in sorted((camp / "attempts").glob("*/attempt-*/measured.jsonl")):
        identity = log.parent.parent.name + ".jsonl"
        if LOG.match(identity):
            attempts.append({**run_row(camp, log, identity=identity, run_dir=log.parent),
                             "attempt": log.parent.name})
    inferred_set = "local-v1" if any(r["arm"].startswith("local-") for r in rows) else "github-v2"
    doc = {
        "campaign": camp.name,
        "octocode_label": args.octocode_label,
        "runner_model": args.runner_model,
        "judge_model": args.judge_model,
        "question_set": args.question_set or manifest.get("question_set", inferred_set),
        "question_ids": manifest.get("question_ids", sorted({r["q"] for r in rows}, key=lambda q: int(q[1:]))),
        "manifest": manifest,
        "runs": rows,
        "prior_attempts": attempts,
        "judgments": judgments(camp),
    }
    out = args.out or PKG / "results" / camp.name / "run.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=1) + "\n")
    readme = out.parent / "README.md"
    if not readme.exists():
        readme.write_text(f"# {camp.name}\n\nBenchmark campaign for `{doc['question_set']}` using `{doc['runner_model']}`.\n\n"
                          f"[Measured runs and judgments]({out.name}): {len(rows)} runs, "
                          f"{sum(bool(r['ok']) for r in rows)} valid, {len(doc['judgments'])} judgments.\n\n"
                          "Invalid attempts and unavailable usage remain in the data; inspect quality before comparing efficiency.\n")
    arms = sorted({r["arm"] for r in rows})
    print(f"wrote {out}: {len(rows)} runs ({', '.join(arms)}), {len(doc['judgments'])} judgments, "
          f"{sum(not r['ok'] for r in rows)} not ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
