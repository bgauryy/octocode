**Answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`, at lines 1837-1838. It limits the walk to the number of existing elements (`ar->count`) when it should limit it to the number of positions (`arLen(ar)`). Each hole in the walked range uses up one step, so the walk ends before it reaches the oldest elements.

**Where**
- `src/t_array.c:1836-1838`:
  ```c
  uint64_t ar_len = arLen(ar);
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
- `src/t_array.c:1854-1870` allocates `collected` with `effective_count` slots. The `while (steps < effective_count)` loop reads `arGet(ar, current_idx)` once per position and steps backward from the anchor. A hole returns NULL but still counts as a step.

**Why elements drop**
- The command walks positions, not only existing items. The comment at `src/t_array.c:1800` says it "may return NULLs".
- The step budget was `ar->count`, the number of existing elements. A hole costs a step but is not an element, so the budget runs out before the walk reaches the oldest position.
- The issue's reproduction is `ARINSERT log a b c d e`, then `ARDEL log 1`. That leaves 4 elements across 5 positions. `ARLASTITEMS log 10` gets only 4 steps, so it returns `nil c d e` and leaves out `a`. Nothing tells the caller the reply is incomplete.

**The fix (PR #15875, merged 2026-09-29, closes #15874)**
I read the patch for `src/t_array.c` only.
1. **Step cap:** `effective_count` is now `min(count, ar_len)`, so the walk covers all positions. The example now returns `a nil c d e`.
2. **Reply limit:** `ARGETRANGE_MAX_ITEMS` is renamed `AR_MAX_REPLY_ITEMS` (1,000,000). `ARLASTITEMS` now replies with the error `count exceeds maximum of %u items` when `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`. This stops a sparse array from producing a huge reply of NULLs. Replies that fit within the existing element count are not limited, so dense arrays behave as before.
3. **Anchor:** the anchor falls back to `ar_len - 1` when `insert_idx >= ar_len` (for example, after the last inserted element was deleted), as well as when it is `AR_INSERT_IDX_NONE`.

**Uncertainty:** The third change is in the PR diff, but the issue doesn't mention it. I did not look at the PR's tests.