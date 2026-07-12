# Bookkeeping & Housekeep

Use this when deciding what upkeep is due. **Bookkeeping = learning** turns outcomes into durable knowledge; **housekeep = cleanup** removes stale state. Both recur during work, and `query workboard` is their shared queue.

## Two concepts, two owners

- **Bookkeeping (learning)** — turn verified outcomes, failures, and corrections into routed, durable knowledge. Ops: `reflect record` (route with `--fix-repo` / `--fix-harness` / `--fix-instructions`), `reflect mine-weakness`, `reflect export-harness`, `reflect developer-review`, `memory record`, `memory record --supersedes <id>`. Deep dive + label→wiki map: `references/learning-loop.md`.
- **Housekeep (cleanup)** — remove or supersede stale locks, signals, terminal refinements, and weak/redundant memories. Ops: reversible `memory archive|restore`; reviewed `maintenance digest`, `lock prune`, `signal prune`, `memory forget`, `refinement delete`; plus automatic salience decay. Deep dive: `references/homeostatic-loop.md`.

Drift sensors feed both: `query workboard` (surfaces `stale_file_refs` + memory-review items), `query files`, `docs staleness`, `verify audit`, `workspace status`.

## Triggers (advisory, workboard-driven)

| When | Bookkeeping (learn) | Housekeep (clean) |
|---|---|---|
| During work | `memory record` a durable fact; `--supersedes <id>` when correcting an old one | — |
| Verified outcome / failure / correction | `reflect record --outcome worked\|partial\|failed` routed to its owner; add `--failure-signature <key>` on failures so they cluster | — |
| Workboard shows items | drain memory-review / `stale_file_refs` rows (supersede or forget) | prune the stale locks/signals the board flags |
| Before finishing, only when sensors show pressure | `reflect mine-weakness` if failures repeated; `export-harness` / `developer-review` when a gap is systemic | `maintenance digest --dry-run`; it reports signal/reference pressure but prunes only expired/superseded memory, expired locks, terminal refinements, and terminal standalone runs |
| End of session / handoff | record only reusable lessons; make routed loop rows terminal | prune expired/resolved rows only when the workboard lists them; dry-run first |
| Idle | — | `maintenance digest --dry-run`, review IDs, then apply |

## Rules

- Dry-run before any mutation; report evidence before removing.
- Delete exact synthetic duplicates and expired handoffs; archive weak old memories. Never delete or mark work successful from age alone: stale unproved runs become `FAILED`.
- Prefer supersession, reversible archive, and decay over destructive deletion; restore is valid only for archived rows, not replacement history.
- Route each lesson to its owner (`references/learning-loop.md`); a loop closes only when applied, verified, and terminal.
- `query workboard` is the upkeep sensor — drain due rows, but create no cleanup work when sensors are clean; then re-run `attend` / `query` to confirm health.
