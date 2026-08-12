# Verify Gate

Load before merging worker output into answers, edits, or commits.

## Principle

Worker output is **claims**, not truth. The orchestrator owns acceptance.
This gate is the skill’s **quality estimator**: pass → integrate; fail → one tighter packet, cascade once to a stronger *installed* model, or finish solo — never silent accept.

## Checklist (all must pass for “pass”)

1. **Parse** — output matches required schema (JSON parses; required fields present).
2. **Ground** — every cited path exists; drop or redo rows with invented paths.
3. **Scope** — no instructions followed outside JOB (no “also refactored…”, no tool talk).
4. **Spot-check** — open 1–2 source slices and confirm the summary/extract is not contradicted.
5. **Confidence** — treat `low` confidence rows as unknowns unless orchestrator re-verifies.
6. **Article / grounded summarize** — every `support_quote` (or equivalent) must be a contiguous substring of the saved INPUT after light whitespace normalize; ungrounded claims → drop or `fail` shard. Target grounded_rate = 1.0 before integrate.

## Verdicts

| Verdict | Meaning | Next |
|---|---|---|
| `pass` | Checklist green | Integrate |
| `partial` | Some shards/rows good | Keep good; redo or solo the rest |
| `fail` | Schema broken or systematic hallucination | One tighter re-packet **or** escalate; **NEVER** silent accept |

## Escalation rules

- After **one** failed retry on the same shard → orchestrator does that shard.
- If **>30%** of shards fail → abort offload for this job; finish solo.
- Security/auth/design content that slipped into worker output → discard; redo on orchestrator.
- Cascade only to **installed** chat models (`ollama list`); never invent a stronger tag.

## Integration hygiene

- Prefer quoting worker facts with path anchors the orchestrator confirmed.
- Do not attribute confidence higher than the spot-check supports.
- In the user-facing report, say what was offloaded and that results were verified (or partially verified).
