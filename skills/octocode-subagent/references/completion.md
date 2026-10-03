# Completion Gate

Load before the parent merges worker results, decides the next spawn, or runs SYNTHESIZE, CLEANUP, or REPORT. Fan-out without a barrier creates false certainty. Worker completion and green focused tests do not prove the integrated user goal.

## Barrier

1. List every live worker; note starting, running, or idle.
2. Wait or poll until each relevant worker is idle or terminal.
3. Stop and remove workers you do not continue.
4. Do not synthesize while a needed worker is still starting or running.

## Merge

1. Collect result packets. Keep `partial` and `blocked` labeled; never average them into "complete".
2. Hunt conflicts first. Disagreement is a finding.
3. Re-check every load-bearing anchor in the parent.
4. Verifier and critic workers start from anchors and acceptance, not from the first worker's prose.
5. Give one parent answer: conclusion, evidence, gaps, next. No worker transcripts.
6. Do not feed unverified worker claims into the next spawn as facts (context poisoning).
7. Compare packet `goal` with returned `result` to catch derailment and withheld information.
8. If the merge needs another research campaign, the cut was wrong (coordination tax).

```text
Barrier: all needed workers idle/terminal
Conflicts: <list or none>
Claims re-checked in parent: <anchors>
Verdict: answer | replan | interview | duck | stop
Gaps: <...>
Next: <one action>
```

If trust is thin, pick the cheapest fitting check in `references/challenge.md` before shipping. Blocked: `references/coordinate.md`.

## Verify

1. Recheck decisive source, test, type, build, security, and runtime anchors in the parent.
2. Run focused checks first, then proportionate package or root checks and real CLI or host paths.
3. When an eval contract exists, compare with the frozen primary, held-out cases, and guardrails.

## Cleanup and documentation

- Remove obsolete implementations, aliases, leftovers, and duplicate policy owners only after you prove deadness and confirm deletion is in scope.
- Preserve user changes and historical evidence. Never rewrite snapshots to hide former behavior.
- Update affected canonical design, command, migration, or runbook docs.
- Re-run checks after cleanup or doc edits that can affect packaging or contracts.

## Report

- Lead with the outcome. Name completed work, exact checks and counts, remaining gaps, destructive actions, and authority still needed.
- Say `partial` when a stop gate remains. Regression evidence is not production or real-host evidence.
- Acceptance needs closed workers and shared work, no authorized proven cleanup left, and an evidence-backed report, or a concrete blocker returned to the requester.

Next: this step ends here.
