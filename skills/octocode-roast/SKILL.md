---
name: octocode-roast
description: "Load this skill first when explicitly asked to roast or harshly critique code or a design. Rank verified defects by impact and give each a pointed critique, repair and proving check."
---

# Octocode Roast

Pin the named files, symbols, diff, PR or branch to a revision or working tree. Ask only if the target is missing. Roast the code, never the coder: sharp and dry by default, aggressive stand-up when requested ("brutal", "harsh" or "no mercy" count). Exaggerate the absurdity, never the evidence.

Think: **What does this promise, and what ordinary situation makes that promise ridiculous?**

## Hunt

1. Find promises and suspect paths with Octocode structure, text and AST searches, LSP callers and history; exact-read source. Whole-repo scope needs a request (else ask, or limit to the named paths): prioritize consequence, fan-in and churn. Exclude generated code unless targeted or exposing a generator defect.
2. Trace input → guards → callers → outcome; check tests, fixtures and docs. Try the strongest innocent explanation before keeping a finding. Exploit, latency and outage claims need a trace/measurement; suspicious syntax stays a lead.
3. Challenge impressive metrics and complexity against the real outcome and a simpler baseline. Rank demonstrated security, data loss, correctness and user-visible speed above style. Deduplicate by root cause; separate unresolved leads with their next check.

The [hunt catalog](references/hunt.md) has probes per promise or failure. Follow every page and continuation before concluding absence; disclose unread scope and partial LSP coverage.

## Rank, write, deliver

Rank, then write each bit as **setup → reversal → concrete consequence** beside its evidence ([rank and write](references/rank-and-write.md)). Return one row per ranked finding:

`**<Critical|High|Medium|Low> · <path:line or design section>.** <trigger> → <mechanism> → <consequence>. Evidence: <trace, run or measurement>. Repair: <smallest fix>. Check: <fails before, passes after>. Confidence: <high|medium|low>. <punchline>`

Then the fix order. Stay read-only unless repairs are requested; [output](output.md) owns repair results and saved reports.

Finish when each finding survives a countercheck, or none holds; report checked scope and unresolved leads. Merge verdicts: `octocode-pr-review`.
