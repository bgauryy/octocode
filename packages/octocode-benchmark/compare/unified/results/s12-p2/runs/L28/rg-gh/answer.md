**Short answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`. It capped the walk by the number of existing elements (`ar->count`) instead of the number of positions (`arLen(ar)`). Each hole used up one step, so the walk stopped before it reached the oldest elements. PR #15875 (merged, "Fix ARLASTITEMS dropping elements past a hole") fixes it.

**Where the bug is**
- At the pinned commit, `src/t_array.c:1836-1838` has:
  ```c
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
- The same function allocates `collected` with `effective_count` slots (about line 1858).
- Its `while (steps < effective_count)` loop calls `arGet(ar, current_idx)` and decrements the index, wrapping at 0 (about lines 1864-1874).
- The loop emits a NULL for each hole but still counts it as a step.

**Why elements are dropped**
- The command is meant to walk positions and return NULL for holes, as its doc comment says.
- The step budget was `ar->count`, which excludes holes.
- In the issue's example, `ARINSERT log a b c d e` followed by `ARDEL log 1` leaves 4 elements across 5 positions. The budget is 4, so the walk covers `e, d, c, (nil)` and stops before `a`.
- The reply is `(nil) c d e`, and nothing tells the caller it is incomplete.

**How the fix changed behavior (PR #15875, which I read via `gh api`; I did not inspect the merged code in the checkout)**
- **Walk length:** The cap is now `min(count, arLen(ar))`. The example now returns `a (nil) c d e`, and the REV form returns `e d c (nil) a`.
- **New hard limit:**
  - `ARGETRANGE_MAX_ITEMS` is renamed `AR_MAX_REPLY_ITEMS` (1,000,000) and is now shared with ARLASTITEMS.
  - ARLASTITEMS returns the error `count exceeds maximum of 1000000 items` when `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`.
  - Dense arrays are not limited, because a big reply there comes from stored elements.
- **Anchor change:** The walk now starts from `ar_len - 1` when `insert_idx` is `AR_INSERT_IDX_NONE` or `>= ar_len`. This covers the case where the last inserted element was deleted. After `ARINSERT a b c d e` and `ARDEL 4`, `ARLASTITEMS 10` now returns `a b c d` instead of `b c d (nil)`.
- **Tests:** The PR adds three tests in `tests/unit/type/array.tcl`: the hole case, the deleted tail, and the sparse-array error.

**Uncertainty:** I did not check the merge commit SHA. The anchor change is not mentioned in the issue; it comes from the PR description and diff.