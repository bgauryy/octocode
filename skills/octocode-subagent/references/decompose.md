# Decompose

Load when splitting a goal into worker-sized units. Why: bad cuts create thrash; good cuts are a DAG.

## Produce a task graph
1. Restate goal + acceptance in one line.
2. List candidate subtasks as verbs with outputs (`probe X → claim ledger`).
3. Mark edges: `blocks` / `feeds` / `conflicts` (same files, same decision).
4. Tag each node **sync-in-parent** vs **async-spawn** — only async gets packets.
5. Collapse anything that needs the same evolving context into one parent step.
6. Cap fan-out (default ≤5). Ask before larger swarms.

## Independence test
A subtask may run in parallel only if all hold:
- Inputs are already known (or cheap to duplicate).
- No write overlap, or exclusive locks assigned.
- Failure of one does not invalidate another's method mid-flight.
- Return shape is mergeable without another research campaign (`synthesize.md`).

## Cut styles
| Style | Use |
|---|---|
| By surface | local vs remote vs package vs web |
| By hypothesis | two competing explanations |
| By layer | data → logic → API (serial if dependent) |
| By role | research / plan / implement / review |
| Map-reduce | many similar probes → parent merge |

Prefer the smallest plan that can satisfy acceptance.

Next: `patterns.md` · `packets.md`.
