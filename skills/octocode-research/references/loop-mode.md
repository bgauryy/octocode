# Loop Mode

Use when a question needs repeated Act→Observe→Learn cycles before the answer is trustworthy: convergence goals, local code-check loops, multi-source research, dead-code proof, or "keep going until evidence converges."

## Iteration Unit

```text
Frame one question -> Act with one cheap call -> Observe status/results -> Learn -> choose next call
```

- **Act:** choose the smallest call that can change the answer. Start discovery/path/symbols/concise; spend exact reads, clone, AST/LSP, PRs, tests, or builds only on surviving leads.
- **Observe:** read `status` first. `empty` = ran but matched nothing; adjust one variable before trusting it. `error` = broken call (auth, validation, rate limit, scope); fix it, never read it as absence.
- **Learn:** update a small ledger: goal, anchors, hypotheses, tried query shapes, cheapest disconfirming step.

### Ledger

Carry anchors forward exactly: paths, lines, match ranges, repository/package/PR ids, branch/ref, cursors, returned `next.*`. Never invent offsets or paths. Keep at least two plausible explanations alive while the answer is unsettled. <!-- style-lint: ignore-line passive-voice -->
Default budget: 3-5 decisive iterations or about 15 minutes for normal tasks. Ask before extending the loop when the next step needs cloning/running code, expensive external research, broad repository scans, or a product/architecture decision.

### Stop Tests

Stop when any is true:
- framed question is answered with grounded evidence and the alternate is killed; <!-- style-lint: ignore-line passive-voice -->
- no cheap next step can change the conclusion;
- iteration/token/wall-clock budget is hit;
- last iterations changed no state.

If a loop stalls on the same `empty`/`error`, change surface or query shape: local ↔ GitHub ↔ npm ↔ history, text ↔ AST ↔ LSP ↔ path, broad ↔ narrow.

### Loop Output

Do not output a transcript. End with: **Answer**, **Evidence**, **Loop trace** (decisive iterations only), **Verification** that ran, **Open gaps**.
Declare `Mode: Loop` when this path owns the run.

Next: when the loop needs campaign framing — budgets, fan-out, subagents — load `references/researcher-mindset.md`. When a surviving lead needs an edit go to `references/workflow-change.md`. When the loop is an accept/revert gate on a skill edit load `references/improve-loop.md`. Otherwise a passed stop test ends the run.
