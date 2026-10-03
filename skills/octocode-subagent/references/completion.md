# Completion Gate

Load before the parent merges worker results, decides the next spawn, or runs SYNTHESIZE, CLEANUP, or REPORT. Green focused tests do not prove the integrated goal.

## Barrier
1. List every live worker and its state (starting, running, idle).
2. Wait or poll until each needed worker is idle or terminal.
3. Stop and remove workers you do not continue.

## Merge
1. Collect result packets; never average `partial` or `blocked` into `complete`.
2. Hunt conflicts first: disagreement is a finding.
3. Compare packet `goal` with returned `result` to catch derailment and withheld information.
4. Never feed unverified claims into the next spawn as facts (context poisoning).

```text
Barrier: all needed workers idle/terminal
Conflicts: <list or none>
Claims re-checked in parent: <anchors>
Verdict: answer | replan | interview | duck | stop
Gaps: <...>
Next: <one action>
```

## Verify
1. Run focused checks, then proportionate package or root checks and real CLI or host paths.
2. With an eval contract, compare against the frozen primary, held-out cases, and guardrails.

## Cleanup
- Remove obsolete implementations, aliases, leftovers, and duplicate policy owners only after you prove deadness and confirm deletion is in scope.
- Preserve user changes and historical evidence; never rewrite snapshots to hide former behavior.
- Update affected canonical design, command, migration, or runbook docs. Re-run checks if cleanup or docs can affect packaging or contracts.

## Report
- Lead with the outcome: completed work, exact checks and counts, gaps, destructive actions, authority still needed.
- Say `partial` while a stop gate remains. Regression evidence is not production or real-host evidence.
- Done means: workers and shared work closed, no authorized proven cleanup left, an evidence-backed report. Otherwise return a concrete blocker.

Next: this step ends here.
