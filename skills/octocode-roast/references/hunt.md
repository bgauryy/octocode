Load when choosing what to ask of the scoped target or which tool answers it.

# Hunt questions and tools

Pick the rows whose promise or failure the target can exhibit and run their proving check; never print the table or ask the user these questions. Follow new evidence beyond this list. An answered question is a lead: it becomes a finding only when its proving check demonstrates the defect.

| Hunt | Questions to ask the code | Proving check |
|---|---|---|
| Promise versus behavior | What do the name, docs and interface promise? Which normal input contradicts that promise? Does “automatic” require a manually maintained inventory? | Trace a documented example through implementation and consumer. |
| Boundary inputs | What happens with empty, missing, zero, duplicate, malformed, Unicode or unexpectedly large input? Does validation protect the actual entry point? | Exercise boundary classes allowed by the contract. |
| Failure honesty | Can failure return success, an empty result or partial data without disclosure? Where does the original error disappear? | Inject a dependency failure; inspect the caller-visible result. |
| State and concurrency | What happens twice, concurrently, after cancellation, or after restart? Can stale cache, shared state or retry change the answer? | Trace ownership and interleavings; test the competing operations. |
| Data integrity | Can an interrupted write corrupt state? Can retries duplicate side effects? Can pagination lose or repeat evidence? | Interrupt at the state transition; reconstruct all selected results. |
| Trust boundaries | Who controls this path, command, query or identifier? Is authorization checked where the resource is accessed? Can logs expose secrets? | Trace untrusted input to the sink and inspect the enforcing guard. |
| Cost and resources | Does one request cause work per item, per caller or per retry? Who releases memory, handles and child processes on every exit? | Check callers and cleanup; profile realistic workloads before claiming a bottleneck. |
| Complexity and ownership | What failure does each abstraction prevent? Is one rule copied across layers? Can the copies disagree? | Find competing owners and demonstrate divergent behavior. |
| Defaults and configuration | What happens when someone follows the instructions exactly? Are defaults valid together? Can interfaces interpret one setting differently? | Run the documented path and compare configuration consumers. |
| Tests and observability | Would tests fail if the behavior broke? Do mocks hide the integration failure? Could operators distinguish empty from failed? | Inspect assertions and real boundaries; reproduce the missing case. |
| Change and history | Did this patch invalidate a caller, invariant or earlier fix? Is suspicious code defending against a known constraint? | Read callers, contract and relevant change history before judging. |
| Deletion and simplification | If this branch, dependency or layer vanished, what observable behavior would change? Is alleged dead code reached indirectly? | Confirm references, entry points and dynamic registration; do not equate no text hits with unused. |

## Pick the tool

Read a tool's schema before its first call; do not invent query fields. Read the checkout's evidence before any remote comparison.

| Need | Tool / next step |
|---|---|
| Find layout, large files or likely entry points | `structureSearch` → `localFetch`; size is a lead, not a defect. |
| Find a promise, error path, setting or duplicate owner | `localSearch` → `localFetch` at matching blocks or ranges; read guards and surrounding control flow. |
| Find structural patterns such as calls inside loops or nested handlers | `astSearch` → exact source; syntax matches remain candidates. |
| Confirm symbol identity, consumers and blast radius | `lspSearch` definitions, references, callers or callees at the source location. If unavailable or partial, corroborate with source. |
| Check actual behavior, working-tree diff or local history | Shell for focused tests, profiler, `git diff` or `git log`; preserve existing changes. |
| Scout evidence too broad for exact searches | `clasify` if enabled → exact-read candidates; classification is not proof or exhaustive coverage. |
| Explain an upstream decision or dependency | Relevant GitHub history tools or `artifactSearch`; use `securityCheck` for known vulnerability claims. |

Batch independent lookups. Apply the [entry’s paging, absence, coverage-disclosure](../SKILL.md#hunt) and [read-only](../SKILL.md#rank-write-deliver) rules.
