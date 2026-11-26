Load when setting loop limits, detecting stalls or repetition, or defining stop conditions.

# Execution control

## Controls

Take cost, token, time and concurrency limits from the caller or host; with none given, state the limit you assumed in the result. Stop at a limit with a Partial return. Track cancellation, checkpoints, retry safety and completion. Budgets bound exposure; evidence decides whether more work is useful. A fixed iteration count alone cannot distinguish progress from repetition.

## Loop types

| Loop | Example | Policy |
|---|---|---|
| Productive | Each page reveals relevant records | Continue within budget |
| Retry | Transient network failure | Retry only errors the tool marks retryable, with backoff; at the caller's retry limit, return Partial |
| Duplicate | Identical calls return identical results | Stop, or change the input or the tool; another identical call with no state change is a defect |
| Circular reasoning | Same hypotheses revisited | Name the observation that would separate the hypotheses and fetch it; if none exists, return Partial |
| Exploration | Searching unrelated files indefinitely | Narrow the scope |
| Repair | Each fix causes a new failure | Re-evaluate the root cause |
| Delegation | Agents delegate to each other repeatedly | Enforce depth and ownership |
| Pagination | Same cursor returned again | Detect the cursor cycle |
| Optimization | Performance changes without measurement | Require a benchmark |
| Verification | Rechecking verified, unchanged state | Reuse the verified result ([design principles](architecture.md#design-principles)) |

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
| Done | Acceptance criteria met and required verification done | Result, evidence, residual risk |
| Blocked (the request stays open: an interruption, not an [outcome](handoffs.md#handoff-protocols)) | A material dependency, permission or input is unavailable | What is needed, from whom |
| Partial | Budget exhausted or no further useful action | Findings, what remains unverified |
