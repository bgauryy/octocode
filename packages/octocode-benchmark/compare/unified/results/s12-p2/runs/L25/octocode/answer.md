**Short answer:** `_.isEqual` avoids infinite recursion with a `Stack` (a map-like cache) that records which object is currently being compared with which. Two cyclic values count as equal only when each one is already on the stack paired with the other. All of this is in `lodash.js` at 2b5e6f7.

**How the stack is threaded through**
- `baseIsEqualDeep` creates `stack || (stack = new Stack)` (`lodash.js:3359`, `3372`, `3379`). It then passes the stack to `equalArrays`, `equalByTag` and `equalObjects` (`3361-3362`, `3380`).
- The stack is passed down on every recursive `equalFunc(...)` call (e.g. `5748`, `5757`, `5903`).

**Arrays (`equalArrays`, `5706-5766`)**
- Before iterating, it looks up both operands: `stack.get(array)` and `stack.get(other)` (`5715-5716`).
- If both are already on the stack, it stops recursing and returns `arrStacked == other && othStacked == array` (`5717-5718`).
- Otherwise it records the pairing in both directions with `stack.set(array, other)` and `stack.set(other, array)` (`5724-5725`). It then compares the elements.
- It removes both entries with `stack['delete']` after finishing (`5763-5764`).

**Plain objects (`equalObjects`)**
- The logic is the same. Both operands are looked up (`5881-5882`).
- If both are present, it returns `objStacked == other && othStacked == object` (`5883-5884`).
- Otherwise it sets the pairing in both directions (`5887-5888`) and deletes it at the end (`5923-5924`).

**Maps and Sets (`equalByTag`, `5826-5840`)**
- The check is one-sided: `stacked = stack.get(object); if (stacked) return stacked == other;` (`5830-5833`). The code comment there says "Assume cyclic values are equal."
- Otherwise it calls `stack.set(object, other)` (`5837`), compares the entries with `equalArrays`, then deletes the entry (`5839`).

**When two self-referencing values are equal**
- When the comparison revisits a pair, it reports equal if the earlier pairing matches. That means `object` was paired with this same `other`, and `other` was paired with this same `object`. The cycle is then treated as consistent and not examined further.
- If either value is paired with a different partner, the result is `false`. For example, one structure cycles back to its parent while the other cycles to a different node.
- If the cyclic pair is consistent, the result for the whole comparison is decided by the remaining, non-cyclic parts.
- Entries are deleted when each comparison finishes. The stack therefore tracks only the current path of comparisons in progress, not everything ever visited.

**Uncertainty:** I did not read lines 5767-5825 or 5841-5880 (the rest of `equalByTag` and the start of `equalObjects`). I also did not run any examples.