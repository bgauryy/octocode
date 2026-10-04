**1. Helped:**
- The single `localSearch` call on `batch_semaphore.rs` (regex `tracing::trace!|trace!\(|tracing::`, contextLines 3) listed every tracing site at once, so I could pick the candidate in `assign_permits`.
- The two `localFetch` line-range reads (262-345 and 530-595) gave the deciding code: the CAS at line 556, the trace at line 561, and the `pop_back` at line 327 that only runs after `assign_permits` returns.

**2. Did not help:**
- The regex was broad and returned noise, such as the span-construction sites and the `Span::none()` lines.
- I did not call `ghGetHistoryItem` or `ghSearchHistory` on issue #8541, so I never checked the issue's own explanation. I also did not read `Acquire::drop` or use `lspSearch` to trace who dereferences the stale node. The "dangling pointer" step is therefore my inference.
- I did not rule out the other trace at line 354 (in the release path), which could also panic.

**3. Next time:**
- Fetch issue #8541 first with `ghGetHistoryItem` to confirm the reported site.
- Read `Acquire::drop` with `localFetch`.
- Run `lspSearch` references on `assign_permits` to confirm it has no other callers.

**4. Confidence:**
- High for the trace location (`batch_semaphore.rs:561`) and for the ordering mechanism: the permits are decremented before the event, and the `pop_back` only happens after `assign_permits` returns.
- Medium for the exact use-after-free consumer, since that step was never verified.