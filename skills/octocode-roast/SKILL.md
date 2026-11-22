---
name: octocode-roast
description: "Load this skill first when explicitly asked to roast or harshly critique code or a design. Rank verified defects by impact and give each a pointed critique, repair and proving check."
---

# Octocode Roast

Pin the named files, symbols, diff, PR or branch to a revision or working tree. Ask only if the target is missing. Roast the code, never the coder: sharp and dry by default, aggressive stand-up when requested ("brutal", "harsh" or "no mercy" count as requests). Exaggerate the absurdity, never the evidence.

Think: **What does this promise, and what ordinary situation makes that promise ridiculous?**

## Hunt

1. Find promises and suspect paths with available Octocode structure/text/AST searches, LSP callers and relevant history; exact-read source. Whole-repo scope requires a request: prioritize consequence, fan-in and churn. Exclude generated code unless targeted or exposing a generator defect.
2. Trace input → guards → callers → outcome; check tests, fixtures and docs. Try the strongest innocent explanation before keeping a finding. Exploit, latency and outage claims need a trace/measurement; suspicious syntax stays a lead.
3. Challenge impressive metrics and complexity against the real outcome and a simpler baseline. Rank demonstrated security, data loss, correctness and user-visible speed above style. Deduplicate by root cause; separate unresolved leads with their next check.

Use the [hunt catalog](references/hunt.md) when you need a probe for a particular promise or failure. Follow emitted pages and continuations before drawing a conclusion about absence. Disclose unread scope and partial LSP coverage.

## Rank, write, deliver

Set severity by impact, reachability and affected scope. Build each bit as **setup → reversal → concrete consequence**, beside its evidence; use [rank and write](references/rank-and-write.md) for severity and delivery examples. Return ranked findings, each with source, trigger, impact, repair and a proving check, then the fix order as finding locations. Critique stays read-only unless repairs are requested. [Output](output.md) owns repair results and optional saved reports.

Finish when each finding survives a countercheck. Report checked scope and unresolved leads. If no defect holds up, say what you reviewed. Use `octocode-pr-review` for a requested merge verdict.
