**Answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`. Lines 1837-1838 cap the walk by the number of existing elements (`ar->count`) instead of the number of positions (`arLen(ar)`). Each hole uses up one step, so the walk stops before it reaches the oldest elements. PR #15875 (merged 2026-09-29) fixed it.

**Where**
- `src/t_array.c:1836-1838`:
  ```c
  uint64_t ar_len = arLen(ar);
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
- That value sizes the `collected` buffer (line 1854) and bounds the `while (steps < effective_count)` loop (lines 1860-1870).

**Why elements are dropped**
- The command walks backward over positions from the anchor and calls `arGet` on each. A hole yields a NULL but still counts as a step (lines 1860-1870). The function comment at lines 1800-1801 says it walks positions and may return NULLs.
- With `count` larger than `ar->count`, the loop runs only `ar->count` times. Any holes in the walked range consume those steps, so the walk ends early.
- The issue's example is `ARINSERT log a b c d e` followed by `ARDEL log 1`. That leaves 4 elements across 5 positions, so `ARLASTITEMS log 10` walks only 4 positions. It returns `nil, c, d, e` and omits `a`, with no signal that the reply is incomplete.

**How the fix changed behavior** (PR #15875, diff to `src/t_array.c`)
1. The cap is now `(uint64_t)count > ar_len ? ar_len : (uint64_t)count`. The command walks up to the full array length, so it returns every position, including holes. The example now returns `a, nil, c, d, e`.
2. It adds an error guard. If `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`, it replies with the error `count exceeds maximum of %u items`. Replies no larger than the number of existing elements are never limited, so dense arrays behave as before.
3. `ARGETRANGE_MAX_ITEMS` is renamed to `AR_MAX_REPLY_ITEMS`, so both commands share the 1,000,000 limit.
4. The anchor is also corrected. If `insert_idx` is `AR_INSERT_IDX_NONE` or is at or past `ar_len`, the walk now anchors at `ar_len - 1`. The diff comment gives the case where the last inserted element was deleted.

**Uncertainty**
- I read the fix from the PR diff, which is a trimmed view. The local checkout is the pre-fix commit 20bb2cfc54, so it still shows the buggy code.
- I did not check any tests the PR may have added.