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

| Control | Why |
|---|---|
| Max steps | Prevent infinite action loops |
| Max tool calls | Bound external operations |
| Max token budget | Control model usage |
| Max cost | Control financial exposure |
| Deadline | Bound end-to-end duration |
| Retry budget | Prevent repeated failures |
| Delegation depth | Avoid recursive agent explosions |
| Concurrency limit | Protect external systems |
| Progress detection | Detect repeated nonproductive behavior |
| No-progress counter | Trigger escalation or termination |
| Cancellation propagation | Stop unneeded downstream work |
| Checkpointing | Keep recoverable state |
| Circuit breaker | Stop calling a repeatedly failing service |
| Approval gate | Control consequential mutations |
| Completion criteria | Define objective success |

Budgets are safety boundaries. Evidence, not an arbitrary call count, decides whether more work is useful.

## Loop types

`max_iterations = 20` is not loop control. The agent must recognize progress, repetition, a failed hypothesis and unnecessary work.

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

```python
def should_continue(state, budget):
    if state.acceptance_criteria_met:
        return False
    if not budget.available():
        return False
    if state.identical_action_count >= 2:
        return False
    if state.no_progress_steps >= 3:
        return False
    if state.has_unrecoverable_error:
        return False
    return True
```

The thresholds are illustrative. Distinguish harmless repeated reads from non-idempotent writes in retry policy, and prefer changing strategy over terminating when a loop stalls but the task is still reachable.

## Stop conditions

| Stop | When | Return |
|---|---|---|
| Success | Acceptance criteria met and required verification done | Result, evidence, residual risk |
| Blocked | A material dependency, permission or input is unavailable | What is needed, from whom |
| Partial | Budget exhausted or no further useful action | Findings, what remains unverified |

## Script first, agent second

If an operation can be a reliable deterministic program, use the program. The agent decides when to run it, with which inputs, and what its output means.

| Task | Executor |
|---|---|
| Count files, symbols or dependencies | Script |
| Parse JSON, YAML or source AST | Parser or script |
| Graph metrics, dependency cycles | Graph algorithm |
| Exact symbol references | LSP or indexer |
| Compare two schemas | Schema diff tool |
| Formatting | Formatter |
| Code defects | Static analyzer |
| Unit and integration tests | Test runner |
| Performance statistics | Script |
| Validate generated output | Schema validator |
| Which architectural concern matters | Agent |
| Competing debugging hypotheses | Agent |
| Next investigation | Agent |
| Migration trade-offs | Agent |
| Explaining findings to an engineer | Agent |
| A missing analysis script | Agent writes it, then runs and validates it |

| Stage | Owner / result |
|---|---|
| Decide what to measure, check or change | Agent: goal and acceptance |
| Select tested automation, or create it for an uncovered need | Agent: inputs and permissions |
| Parse, compile, test, calculate graph metrics or query | Deterministic executor |
| Return counts, findings, errors, provenance and references | Structured result and artifact |
| Interpret, verify and choose the next action | Agent |

Scripts give reproducible, testable behavior, exact arithmetic and enumeration, lower cost for repeated work, transparent failures, reusable results and no fabricated counts or skipped cases.

They do not resolve ambiguous intent, choose between conflicting goals, or prove a design appropriate. Generated scripts can be buggy: for destructive operations validate inputs, constrain permissions, inspect effects and get approval first.

Rule: existing tested script first, a new script when worthwhile, model reasoning for decisions that cannot reliably become deterministic processing.

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
