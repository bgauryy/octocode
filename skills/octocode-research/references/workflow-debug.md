# Workflow: Debug / Root Cause

Use when `problem-framing.md` classifies a supported contract violation as a bug, or when an unknown symptom needs investigation.
Read `algorithm.md` first for routing and evidence grades.
Use `code-research.md` for the proof ladder on any code claim raised here.

```text
problem contract: actual + expected + authority + trigger + impact
-> capture reproduction/equivalent runtime evidence and symptom anchor
-> map entry -> transformations -> state/dependencies -> output/consumers
-> two hypotheses: likely mechanism + plausible alternate
-> find first boundary where actual diverges from the named contract
-> exact reads around the boundary; AST/LSP/history/tests for reachability and "why now"
-> disconfirm the alternate; seek counterfactual proof that removing/changing the cause prevents the symptom
-> root cause receipt: mechanism + trigger + violated contract + divergence boundary + why now
```

The nearest suspicious line, recent commit, or correlation is not root cause.
If reproduction is unavailable, name the equivalent evidence and cap confidence.
If both hypotheses survive, ask for the missing runtime input, log, or config.

Keep the root cause answer tight:

```text
Root cause: <mechanism and trigger>
Violated contract: <authority and divergence boundary>
Evidence: <path:line / runtime output / semantic or structural proof>
Disconfirmation: <alternate killed and counterfactual/result>
Why now: <change, input, dependency, config, or data condition>
Fix: <smallest safe repair or decision needed>
Verification: <test/build/search/history check run or still needed>
```

If the fix requires an edit, hand off to `workflow-change.md` for blast-radius and patch discipline.

Validate: `node scripts/eval-research.mjs --case code-investigation` (general root-cause shape) or `--case oql-graph-proof` (dead-code/reachability root causes).
