Load when one traversal actor must inspect a declared whole-codebase and running-application scope with breadth-first and depth-first traversal.

# Single-actor BFS/DFS audit

The orchestrator assigns one traversal actor the complete audit loop. It owns discovery, trace execution, authorized edits, rebuild requests, and observations; it returns them to the orchestrator's ledger. Do not fan out or spawn workers in this workflow.

```mermaid
flowchart TD
  S[Pin scope and identity] --> B[BFS inventory of surfaces and junctions]
  B --> Q[Queue every discovered path]
  Q --> D[DFS one path to observable outcome]
  D --> V{Check valid?}
  V -- yes --> N[Record row and next branch]
  V -- stale or unknown --> R[Diagnose and replan]
  R --> D
  N --> M{Inventory reconciled?}
  M -- no --> Q
  M -- yes --> O[Return coverage and limits]
```

## Traverse

- Pin roots, revision, existing uncommitted work, entry points, configuration, runtime target, and exclusions. Inventory manifests, registrations, schemas, exports, configuration, live catalogs, adapters, jobs, callbacks, and lifecycle entry points.
- BFS every junction: branch, dispatch, adapter, shared service, fan-in/out, or ownership/state/control boundary. Rank critical paths for order, never exclusion. Dedupe by resolved identity plus bounded configuration/scenario identity; one visited file never covers every incoming contract.
- DFS `input → validation → dispatch → implementation → state/dependency → output → consumer`, including supported success, denial, empty, partial, failure, cancellation, retry, timeout, cleanup, and dynamic/registered paths. Keep the pending BFS frontier and DFS stack/resume point beside the ledger for interruption and replan.
- Use Research for live schemas and continuations; use [AST/LSP](ast-lsp-workflows.md) for candidates and semantic checks; use Engineer's [review angles](../../octocode-engineer/references/checks.md#review-angles) and [change discipline](../../octocode-engineer/references/change-discipline.md) for authorized code work.
- For runtime rows use the [identity guard](verification.md#automatically-check-artifact-identity). Source-only rows name their source/index scope. Missing fixtures, access, services, or tools remain explicit gaps.

## Repair and completion

The actor reports a proven boundary and focused evidence; the orchestrator decides authorized change sequencing and requeues affected callers, consumers, junctions, and scenarios after invalidation. Stop condition and claims:

| Claim | Holds only when |
|---|---|
| Traversal exhausted | The BFS/DFS frontier is empty and every discovered surface is accounted for in the ledger |
| Full verification | Exhausted, source and live inventories reconcile, and every applicable row is `verified` with its bounded scenario/configuration identity; `queued`, `failed`, `unverified`, or `stale` rows prevent the claim |
| “All checked” | It covers the declared inventory and scenarios, never a visited-file set or arbitrary sample |

Next: apply [verification](verification.md) and return the domain result through the orchestrator.
