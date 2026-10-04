**Short answer:** The bug is in `arlastitemsCommand` in `src/t_array.c`. It capped the walk by the number of existing elements instead of the number of positions. The fix is PR #15875 (merged, "Fix ARLASTITEMS dropping elements past a hole"), which caps by `arLen(ar)` instead.

**Where (at 20bb2cfc54)**
- `src/t_array.c:1837-1838`:
  ```c
  uint64_t effective_count =
      (uint64_t)count > ar->count ? ar->count : (uint64_t)count;
  ```
- The walk loop that uses it is `while(steps < effective_count)` at `src/t_array.c:1860`.
- The command's own comment (`src/t_array.c:1796-1800`) says it walks positions and may return NULLs.

**Why it drops elements**
- `ar->count` is the number of existing elements. The loop walks backward over positions from the insert anchor, one position per step, and a hole costs a step just like an element does.
- With `a b c d e` and position 1 deleted, `ar->count` is 4, so only 4 positions are walked (`e d c <hole>`). Position 0 (`a`) is never reached, so the reply is `(nil) c d e`, with no error.

**How the fix changed behavior** (diff of PR #15875)
1. **Walk length:** `effective_count` is now `min(count, arLen(ar))`. For the example, `ARLASTITEMS log 10` returns `a (nil) c d e`, and with `REV` it returns `e d c (nil) a`.
2. **New error:** the constant `ARGETRANGE_MAX_ITEMS` is renamed `AR_MAX_REPLY_ITEMS` (1,000,000), and ARGETRANGE keeps using it. ARLASTITEMS now replies `count exceeds maximum of 1000000 items` when `effective_count > ar->count && effective_count > AR_MAX_REPLY_ITEMS`. The limit applies only to sparse walks. A dense array can still return more than 1M elements, up to its element count.
3. **Anchor fix (not in the issue):** the walk now starts at `ar_len - 1` when `insert_idx` is `AR_INSERT_IDX_NONE` or `>= ar_len`. After `ARINSERT a b c d e` and `ARDEL 4`, `ARLASTITEMS 10` now returns `a b c d` instead of `b c d (nil)`.
4. **Tests:** three tests were added in `tests/unit/type/array.tcl`, one each for the hole case, the deleted last element, and the sparse error.

**Uncertainty:** I read the fix from the PR diff on GitHub, not from a local checkout of the fixed commit. The local checkout is the pre-fix code.