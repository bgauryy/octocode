**Short answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`. It caps the walk by the number of stored elements (`ar->count`) instead of the number of positions (`arLen(ar)`). Each hole uses up one of those steps, so the walk stops before it reaches the oldest elements. The fix is PR #15875, merged as 790d459f09. It caps by `arLen`, adds an error limit, and corrects the starting position.

**Where the bug is**
- `src/t_array.c:1837-1838` at 20bb2cfc54:
  ```c
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
- The command starts at `anchor_idx` and walks backward with `while(steps < effective_count)`, which is in the same function at about lines 1860-1870. Each step calls `arGet`, which returns NULL for a hole.
- The command comment at `src/t_array.c:1795-1801` says the command walks positions and may return NULLs, so the cap contradicts it.

**Why elements get dropped**
- `ar->count` is the number of existing elements, and `arLen(ar)` is the span of positions. On a sparse array `count < arLen`.
- Take `ARINSERT log a b c d e`, then `ARDEL log 1`, then `ARLASTITEMS log 10`. There are 5 positions but only 4 elements, so `effective_count` is 4.
- The walk visits e, d, c and the hole, which is 4 steps. It stops before reaching `a`, and the reply is `(nil) c d e`. Nothing signals that the reply is incomplete (issue #15874).

**How the fix changed behavior** (diff of PR #15875, which I read through the GitHub API; the local checkout is still pre-fix)
1. **Walk length:** `effective_count` is now `min(count, arLen)`. The reply contains NULLs for holes, so the example returns `a (nil) c d e`.
2. **New error limit:** `ARGETRANGE_MAX_ITEMS` is renamed `AR_MAX_REPLY_ITEMS` (1,000,000). ARLASTITEMS now returns the error `count exceeds maximum of 1000000 items` when `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`. Dense arrays and any reply no larger than the number of stored elements are not limited.
3. **Anchor change (not in the issue):** the walk now starts from `ar_len - 1` when `insert_idx` is `AR_INSERT_IDX_NONE` or `insert_idx >= ar_len`. For example, after `ARINSERT myarray a b c d e` and `ARDEL myarray 4`, `ARLASTITEMS myarray 10` returns `a b c d` instead of `b c d (nil)`.
4. **Tests:** three tests were added to `tests/unit/type/array.tcl`, covering the hole case, the deleted last element, and the error on a sparse array.

**Uncertainty:** I didn't build or run Redis, so the example outputs come from the issue, the PR description and the PR's tests. I haven't reproduced them.