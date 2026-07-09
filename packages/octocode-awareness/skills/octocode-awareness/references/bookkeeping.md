# Bookkeeping & Housekeep

Use this when deciding what upkeep is due. **Bookkeeping = learning** turns outcomes into durable knowledge; **housekeep = cleanup** removes stale state. Both recur during work, and `query workboard` is their shared queue.

## Two concepts, two owners

- **Bookkeeping (learning)** — turn verified outcomes, failures, and corrections into routed, durable knowledge. Ops: `reflect record` (route with `--fix-repo` / `--fix-harness` / `--fix-instructions`), `reflect mine-weakness`, `reflect export-harness`, `reflect developer-review`, `memory record`, `memory record --supersedes <id>`. Deep dive + label→wiki map: `references/learning-loop.md`.
- **Housekeep (cleanup)** — remove or supersede stale locks, signals, refinements, and weak/redundant memories. Ops: `maintenance digest`, `lock prune`, `signal prune`, `memory forget`, `refinement delete`, plus automatic salience decay. Deep dive: `references/homeostatic-loop.md`.

Drift sensors feed both: `query workboard` (surfaces `stale_file_refs` + memory-review items), `query files`, `docs staleness`, `verify audit`, `workspace status`.

## Triggers (advisory, workboard-driven)

| When | Bookkeeping (learn) | Housekeep (clean) |
|---|---|---|
| During work | `memory record` a durable fact; `--supersedes <id>` when correcting an old one | — |
| Verified outcome / failure / correction | `reflect record --outcome worked\|partial\|failed` routed to its owner; add `--failure-signature <key>` on failures so they cluster | — |
| Workboard shows items | drain memory-review / `stale_file_refs` rows (supersede or forget) | prune the stale locks/signals the board flags |
| Before finishing (post-verify) | `reflect mine-weakness` if failures repeated; `export-harness` / `developer-review` when a gap is systemic | `maintenance digest --dry-run`; `memory forget --dry-run` weak/stale rows |
| End of session / handoff | ensure lessons recorded and loop rows terminal | `lock prune --expired-only`, `signal prune --resolved` (dry-run first) |
| Idle | — | `maintenance digest --dry-run`, review IDs, then apply |

## Rules

- Dry-run before any mutation; report evidence before removing.
- Prefer supersession and decay over destructive deletion.
- Route each lesson to its owner (`learning-loop.md`); a loop closes only when applied, verified, and terminal.
- `query workboard` is the to-do list — drain due upkeep before concluding, then re-run `attend` / `query` to confirm health.
