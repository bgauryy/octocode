# Output
Load when presenting an eval or loop result. Why: incomplete reports invite vibe acceptance.

## Loop report
```markdown
## Goal
## KPI
- primary (lagging): <name> (<dir>) baseline=… result=… target=…  [serves goal]
- leading (optional): …
- guardrails: …
## Loop level and budget
experiment | suite | meta; trials/cost
## Subject and harness
what changed; whether cases/graders stayed frozen
## Checks run
- command + exit code / score
- held-out: …
## Verdict
ACCEPT | REVERT | CONTINUE  (inner-loop KEEP maps to ACCEPT, DISCARD to REVERT)
## Next (when needed)
```

Validate with `scripts/loop-report.mjs` before claiming done.
Write run artifacts (answers, grades, loop reports) under `.octocode/` in the workspace — never a session temp dir; only permanent suite files live in `evals/`.

For multi-iteration runs, add a short retrospective: iterations, hypotheses kept/killed, metric trajectory, and any escalation.

## Confidence markers
| Marker | Minimum |
|---|---|
| strong | deterministic check or calibrated multi-trial result |
| moderate | one solid grader + corroboration |
| weak | single LLM score, saturated public bench, or narrative only |

Lead with verdict + primary delta. Expand tables only when contested.

Next: route capture → `routing.md`.
