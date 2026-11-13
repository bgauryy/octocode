# Execution control

Load when setting loop limits, detecting stalls or repetition, defining stop conditions, deciding what a script should compute instead of the model, or correcting patchy coding behavior.

## Contents

- [Controls](#controls)
- [Loop types](#loop-types)
- [Progress](#progress)
- [Stop conditions](#stop-conditions)
- [Script first, agent second](#script-first-agent-second)
- [Patchy behavior](#patchy-behavior)

## Controls

Set authorized cost, token, time and concurrency boundaries where the workload needs them; track cancellation, checkpoints, retry safety and completion. Budgets bound exposure; evidence decides whether more work is useful. A fixed iteration count alone cannot distinguish progress from repetition.

## Loop types

| Loop | Example | Policy |
|---|---|---|
| Productive | Each page reveals relevant records | Continue within budget |
| Retry | Transient network failure | Bounded retry with backoff |
| Duplicate | Identical calls return identical results | Stop or change approach |
| Circular reasoning | Same hypotheses revisited | Require new evidence |
| Exploration | Searching unrelated files indefinitely | Narrow the scope |
| Repair | Each fix causes a new failure | Re-evaluate the root cause |
| Delegation | Agents delegate to each other repeatedly | Enforce depth and ownership |
| Pagination | Same cursor returned again | Detect the cursor cycle |
| Optimization | Performance changes without measurement | Require a benchmark |
| Verification | Rechecking verified, unchanged state | Reuse the verified result |

## Progress

Progress signals:

- New relevant evidence found, or an uncertainty resolved.
- An acceptance criterion satisfied.
- A failing test reproduced or fixed.
- A blocked operation unblocked.
- A valid intermediate artifact produced.
- An important hypothesis dismissed.

A larger output payload is not progress.

Continue while a new observation or action can advance acceptance within scope and budget. On repeated unchanged results, classify the failure and change strategy. Repeated reads differ from non-idempotent writes: reconcile uncertain effects before retrying; do not stop reachable useful work merely because a counter elapsed.

## Stop conditions

| Stop | When | Return |
|---|---|---|
| Success | Acceptance criteria met and required verification done | Result, evidence, residual risk |
| Blocked | A material dependency, permission or input is unavailable | What is needed, from whom |
| Partial | Budget exhausted or no further useful action | Findings, what remains unverified |

## Script first, agent second

If an operation can be a reliable deterministic program, use the program. The agent decides when to run it, with which inputs, and what its output means.

Use tested parsers, LSP/indexers, graph algorithms, schema validators, formatters, analyzers, test runners and profilers for exact work. The agent chooses the question, inputs, permissions and acceptance, then interprets the structured result and provenance. Write reusable automation only when an existing tool does not cover the need, and run it to validate behavior.

Scripts do not resolve ambiguous intent or prove a design appropriate. For destructive operations validate inputs, constrain permissions, inspect effects and obtain the applicable authorization.

## Patchy behavior

Patchy agents make local fixes without understanding the system, repeat calls, lose state or make inconsistent changes.

| Anti-pattern | Failure | Better |
|---|---|---|
| Patch-first coding | Fix before understanding the cause | Reproduce and trace behavior |
| Shotgun editing | Speculative edits in many files | Find the minimal causal change |
| Happy-path fix | Edge cases ignored | Verify invariants and failures |
| Blind retry | Same failed call repeated | Classify the error before retrying |
| Context thrashing | Information reloaded repeatedly | Persist evidence and checkpoints |
| Tool thrashing | Switching tools without progress | Name an explicit information goal |
| Speculative refactoring | Scope expands | Keep changes bounded |
| Duplicate policy logic | Competing implementations | One policy owner |
| Fake completion | Success claimed from plausibility | Run the verification |
| Over-delegation | Coordination overhead | Delegate only separable work |
| Over-compaction | Important task state lost | Preserve invariants and provenance |
| Prompt accretion | Endless rules and exceptions | Consolidate into a coherent policy |
| Unbounded exploration | Searching indefinitely | Sufficiency and budget gates |
| Premature optimization | Optimizing without evidence | Profile and benchmark |
| Architecture drift | Module boundaries violated | Architecture tests |

Workflow:

1. **Understand:** the problem, acceptance criteria and constraints.
2. **Inspect:** ownership, boundaries, call paths and relevant source.
3. **Hypothesize:** the likely cause and a plausible rival.
4. **Reproduce:** a failing test or observable baseline.
5. **Change:** the smallest coherent correction.
6. **Verify:** relevant checks, plus a look for unintended consequences.
7. **Return:** changed artifacts, evidence and remaining risks.
