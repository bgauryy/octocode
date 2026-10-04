**Answer:** `_.isEqual` avoids infinite recursion by keeping a `Stack` that maps each object currently being compared to its counterpart. When it meets a pair it is already comparing, it stops recursing and answers from that stack instead. Two cyclic values count as equal only if the cycle pairs up the same way on both sides.

**Mechanism** (all in `lodash.js` at the pinned commit; I couldn't run `git rev-parse HEAD` in this sandbox, so I didn't confirm the checkout is at 2b5e6f7399):
- `baseIsEqualDeep` creates the stack lazily with `stack || (stack = new Stack)` (`lodash.js:3359`, `3372`, `3379`). It passes the stack to `equalArrays`, `equalByTag` and `equalObjects` (`3359`–`3362`, `3379`–`3381`). The unwrapped `__wrapped__` path also passes it through `equalFunc` (`3372`–`3373`).
- **Arrays and typed arrays:** `equalArrays` looks up both values in the stack (`5715`–`5716`). If both are already there, it returns `arrStacked == other && othStacked == array` (`5717`–`5719`). Otherwise it records `stack.set(array, other)` and `stack.set(other, array)` (`5724`–`5725`), compares the elements, and removes both entries at the end (`5763`–`5764`).
- **Plain objects:** `equalObjects` does the same. It checks `objStacked && othStacked` and returns `objStacked == other && othStacked == object` (`5881`–`5884`). It then sets both entries (`5887`–`5888`) and deletes them at the end (`5923`–`5924`).
- **Maps and Sets:** `equalByTag` uses a one-directional check. It does `stacked = stack.get(object)`, and if that is set it returns `stacked == other` (`5830`–`5833`), with the comment "Assume cyclic values are equal". Otherwise it does `stack.set(object, other)`, calls `equalArrays` on the converted entries, and then deletes the entry (`5837`–`5839`).

**When two self-referencing values are equal:**
- If both values are already on the stack, they are equal only when each one is the partner recorded for the other. In other words, `a` was paired with `b` and `b` with `a`. If the cycles are structured differently, for example `a` points back to `b` but `b` points to some other value, it returns `false`.
- When that check passes, the cyclic branch returns `true` without comparing further, so the cycle is treated as equal. The overall result is still `true` only if every other property and element also matches.
- For Map and Set, only the `object` side is checked: a prior entry for `object` must map to `other`.
- Entries are deleted after each comparison finishes. The stack therefore tracks only the current path of comparisons, not every value seen so far.

**Uncertainty:** I read the code but did not run any cyclic examples to confirm the behavior.