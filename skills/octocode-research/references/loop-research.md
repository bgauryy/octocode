# Loop Research

Read this when a technical or code question needs repeated Act→Observe→Learn cycles before the answer is trustworthy. Use it for clear goals that need convergence, local code-check loops, multi-source research, dead-code proof, or "keep going until evidence converges."

## Iteration Unit

Each iteration is:

```text
Frame one question -> Act with one cheap call -> Observe status/results -> Learn -> choose next call
```

- **Act:** choose the smallest call that could change the answer. Start discovery/path/symbols/concise, then spend exact reads, clone, AST/LSP, PRs, tests, or builds only on surviving leads.
- **Observe:** read `status` first. `empty` means the call ran and matched nothing; adjust one variable before treating it as evidence. `error` means fix the call, auth, validation, rate limit, or scope; do not treat it as not-found.
- **Learn:** update a small ledger: goal, anchors, hypotheses, tried query shapes, and the cheapest disconfirming step.

## Ledger

Carry anchors forward exactly: paths, lines, match ranges, repo/package/PR ids, branch/ref, cursors, and returned `next.*`. Never invent offsets or paths.

Keep at least two plausible explanations alive while the answer is unsettled. A single good-looking snippet is a lead, not a conclusion.

## Stop Tests

Stop when any is true:

- the framed question is answered with grounded evidence and the alternate is killed;
- no cheap next step can change the conclusion;
- the iteration/token/wall-clock budget is hit;
- the last iterations changed no state.

If a loop stalls on the same `empty`/`error`, change surface or query shape: local ↔ GitHub ↔ npm ↔ history, text ↔ AST ↔ LSP ↔ path, broad ↔ narrow.

## Finding Checks

For workspace code findings, promote a candidate only after a deterministic check confirms it: exact read, AST/structural match, LSP references/callers/definition, history, build, test, typecheck, or lint. A universal claim is disproven by one counterexample anchor.

## Output

Do not output a transcript. End with: **Answer**, **Evidence**, **Loop trace** with only decisive iterations, **Verification** that actually ran, and **Open gaps**.
