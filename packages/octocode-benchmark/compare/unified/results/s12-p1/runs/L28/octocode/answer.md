**Answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`. It caps the walk by the number of existing elements rather than the number of positions, so holes use up steps and the oldest elements fall off the end of the reply.

**Where**
- `src/t_array.c:1837-1838`:
  ```c
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
  `ar->count` is the number of existing elements.
- The walk loop at `src/t_array.c:1860-1870` runs `while (steps < effective_count)`. It calls `arGet` on each position, going backward from the anchor.
- The `arGet` result is stored even when the position is a hole, which comes out as NULL. The reply length is `steps` (line 1873).

**Why elements are dropped**
- The command walks positions, as its comment says (`src/t_array.c:1800`). Holes appear in the reply as NULLs.
- Each hole still counts as one step, but the step budget is `ar->count`, which doesn't include holes.
- With `ARINSERT a b c d e`, then `ARDEL 1`, there are 5 positions but only 4 elements. `ARLASTITEMS log 10` gets a budget of 4, so it covers `e, d, c, nil` and stops before `a`.
- The reply has no marker saying it is incomplete. The issue's repro shows `(nil) c d e` with `a` missing.

**How the fix changed behavior** (PR #15875, merged 2026-09-29, closing #15874; I read only the `src/t_array.c` patch)
1. **Cap by array length.** The cap is now `arLen(ar)`: `(uint64_t)count > ar_len ? ar_len : (uint64_t)count`. In the repro, `ARLASTITEMS log 10` returns all 5 positions: `a, nil, c, d, e`.
2. **New reply limit on sparse arrays.** The patch renames `ARGETRANGE_MAX_ITEMS` to `AR_MAX_REPLY_ITEMS` (1,000,000), shared with ARGETRANGE. If `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`, the command now returns the error `count exceeds maximum of %u items`. Dense arrays, where the reply fits within the existing elements, are not limited.
3. **Anchor fix.** If `insert_idx >= ar_len`, for example after the last inserted element was deleted, the anchor now falls back to `ar_len - 1`. Before, only `AR_INSERT_IDX_NONE` triggered that fallback.

**Uncertainty:** The local checkout is at the pre-fix commit, so the fix lines come from the PR patch rather than from reading the fixed file. The patch view elided some context lines. I did not run any tests.