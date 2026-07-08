# Workflow: Debug / Root Cause

Use when behavior changed, a test failed, an error appears, or a bug exists.
Read `algorithm.md` first for routing and evidence grades.
Use `code-research.md` for the proof ladder on any code claim raised here.

```text
capture reproduction when available: failing command, log, input, stack frame, endpoint, or changed behavior
-> symptom anchor: error string, failing test, endpoint, file, stack frame, or changed behavior
-> two hypotheses: likely cause + plausible alternate
-> trace entry -> transformation -> failing/changed contract
-> exact reads around each boundary
-> AST/LSP/history/tests to disconfirm one hypothesis
-> mechanism + trigger + fix surface
```

Keep the root cause answer tight:

```text
Root cause: <mechanism and trigger>
Evidence: <path:line / PR / command output>
Why now: <change, input, dependency, config, or data condition>
Fix: <smallest safe repair or decision needed>
Verification: <test/build/search/history check run or still needed>
```

Do not stop at "probably X" when a cheap disconfirming read exists. If both hypotheses survive, ask for the missing runtime input/log/config instead of pretending certainty.

If the fix requires an edit, hand off to `workflow-change.md` for blast-radius and patch discipline.

Validate: `node scripts/eval-research.mjs --case code-investigation` (general root-cause shape) or `--case oql-graph-proof` (dead-code/reachability root causes).
