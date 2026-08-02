# Output
Load when presenting an eval or loop result. Why: incomplete reports invite vibe acceptance.

## Loop report (required sections)
```markdown
## Goal
## KPI
- primary (lagging): <name> (<dir>) baseline=… result=… target=…  [serves goal]
- leading (optional): …
- guardrails: …
## Loop level
experiment | suite | meta
## Budget / trials
## Subject changed
## Harness unchanged? (yes/no)
## Checks run
- command + exit code / score
- held-out: …
## Transcript note
## Verdict
ACCEPT | REVERT | CONTINUE  (inner-loop KEEP maps to ACCEPT, DISCARD to REVERT)
## Next
```

Validate with `scripts/loop-report.mjs` before claiming done.
Write run artifacts (answers, grades, loop reports) under `.octocode/` in the workspace — never a session temp dir; only permanent suite files live in `evals/`.

## Loop retrospective (multi-iteration runs)
After the final verdict, add one short block: iterations run · hypotheses kept/killed ·
metric trajectory (baseline → … → final) · where the loop stalled or escalated (suite/meta).

## Confidence markers
| Marker | Minimum |
|---|---|
| strong | deterministic check or calibrated multi-trial result |
| moderate | one solid grader + corroboration |
| weak | single LLM score, saturated public bench, or narrative only |

Lead with verdict + primary delta. Expand tables only when contested.

Next: route capture → `routing.md`.
