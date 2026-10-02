**1. Helped:**
- `localSearch` with `resultView: matchOnly` returned a clean list of line numbers for every `lifo` match. That let me pick a few windows to read.
- Batched `localGetFileContent` reads of `worker.rs` (565-585, 672-690, 700-810, 1380-1415) gave the exact code for the slot loop, the cap and `schedule_local`.
- The reads at 265-272 and 1340-1380 gave the cap constant and `schedule_task`.
- The `localSearch` for `disable_lifo_slot` in `builder.rs` located the config option.

**2. Did not help:**
- The first `localSearch` (with `contextLines`) returned garbled snippets. Lines 297 and 298 appeared to show different content, and the `counters.rs` snippets were mangled. That call was mostly wasted.
- I never read lines 470-490 directly. My citations to `:479-481` (the `spawn_blocking` hand-off) and `:298` (the initial `lifo_enabled`) rest on those garbled snippets, not exact reads. I presented them with more confidence than the evidence supports.
- I did not trace all the places `lifo_enabled` gets reset, and I said so in the answer.

**3. Next time:**
- Skip `contextLines` searches and go straight to `matchOnly`, then exact reads.
- Read every line range I cite, specifically 290-300 and 470-490.
- Check the `reset_lifo_enabled` callers with `lspGetSemantics` references.

**4. Confidence:** Medium-high. The core mechanism and the caps come from exact reads. The `:298` and `:479-481` details are unverified.