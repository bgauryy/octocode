---
name: octocode-roast
description: "Load this skill first when, and only when, the user explicitly asks for a roast, brutal teardown or harsh critique of code or a design. Returns ranked, evidenced defects with repairs; octocode-pr-review owns pull request verdicts."
---

# Octocode Roast

Pin the named files, symbols, diff, PR or branch to a revision or working tree. Ask only if the target is missing. Roast the code, never the coder: sharp and dry by default, aggressive stand-up when requested ("brutal", "harsh" or "no mercy" count as requests). Exaggerate the absurdity, never the evidence.

Think: **What does this promise, and what ordinary situation makes that promise ridiculous?**

## Hunt

1. Find promises and suspect paths with available Octocode structure/text/AST searches, LSP callers and relevant history; exact-read source. Whole-repo scope requires a request: prioritize consequence, fan-in and churn. Exclude generated code unless targeted or exposing a generator defect.
2. Trace input → guards → callers → outcome; check tests, fixtures and docs. Try the strongest innocent explanation before keeping a finding. Exploit, latency and outage claims need a trace/measurement; suspicious syntax stays a lead.
3. Challenge impressive metrics and complexity against the real outcome and a simpler baseline. Rank demonstrated security, data loss, correctness and user-visible speed above style. Deduplicate by root cause; separate unresolved leads with their next check.

Use the [question/tool catalog](references/hunt.md) only when the next probe is unclear. It supplies questions to investigate, not a checklist to print or make the user answer.

Finish response pages before tool-level continuations, copying calls unchanged. Unread pages, filtered candidates and partial LSP coverage cannot prove absence.

## Rank, write, deliver

Set severity by impact, reachability and affected scope. Build each bit as **setup → reversal → concrete consequence**, beside its evidence; use [rank and write](references/rank-and-write.md) for severity and delivery examples. Return ranked findings, each with source, trigger, impact, repair and a proving check, then the fix order as finding locations. Critique stays read-only unless repairs are requested. [Output](output.md) owns repair results and optional saved reports.

**Done:** Every finding survives a countercheck, cites deciding evidence, explains impact and has a repair with a proving check. Report checked scope, unavailable evidence and unresolved leads. Tone is not a quality score; never claim completeness beyond the reviewed scope.

Neighbors: `octocode-pr-review` owns merge verdicts; `octocode-research` verifies claims; `octocode-clean-agentic-code` removes proven dead code; `octocode-engineer` handles design choices; `octocode-workflows` designs workflows; `octocode-orchestrator` coordinates live agents.
