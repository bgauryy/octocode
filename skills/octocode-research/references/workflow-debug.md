# Debug and root cause

Load when a supported contract is violated or an unknown symptom needs investigation.

```text
contract: actual + expected + authority + trigger + impact
-> reproduction or equivalent runtime evidence + symptom anchor
-> entry -> transformations -> state/dependencies -> output/consumers
-> two hypotheses: likely mechanism + plausible alternate
-> first boundary where actual diverges; exact reads there
-> AST/LSP/history/tests for reachability and "why now"
-> disconfirm the alternate; counterfactual: removing the cause removes the symptom
```

The nearest suspicious line, a recent commit, or a correlation is not root cause. Without reproduction, name the equivalent evidence and cap confidence; if both hypotheses survive, ask for the missing input, log, or config.

Report: `Root cause · Violated contract · Evidence (path:line / runtime) · Disconfirmation · Why now · Fix · Verification`.

Next: fix → `workflow-change.md`; upstream mechanism → `workflow-external.md`; hypotheses still tied → `campaigns.md`.
