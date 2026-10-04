**Short answer:** `_.isEqual` keeps a `Stack` that maps each object being compared to its counterpart. If it meets a pair that is already being compared, it stops recursing. It treats the pair as equal only if each object is mapped to the other one in that stack. Everything else about the two values must still match.

**Mechanism**
- `baseIsEqualDeep` creates the stack with `stack || (stack = new Stack)` (`lodash.js:3359`, `3372`, `3379`). It passes the stack to `equalArrays`, `equalByTag` and `equalObjects` (`3361-3362`, `3380`). Those functions pass it on to every recursive `equalFunc` call (for example `5748`, `5757`, `5903`).
- **Arrays** (`equalArrays`, `5715-5725`):
  - It reads `stack.get(array)` and `stack.get(other)`.
  - If both are present, it returns `arrStacked == other && othStacked == array` (`5718`).
  - Otherwise it records `stack.set(array, other)` and `stack.set(other, array)`.
  - It removes the `other` entry when it finishes (`stack['delete'](other)`, ~`5766`).
- **Objects** (`equalObjects`, `5881-5888`):
  - It uses the same two-way check: `objStacked == other && othStacked == object` (`5884`).
  - It records the pair in both directions.
  - It deletes both entries at the end (`5916-5917`).
- **Maps and Sets** (`equalByTag`, `5830-5838`):
  - It reads `stack.get(object)` and returns `stacked == other` if there is one. The code comment is "Assume cyclic values are equal."
  - Otherwise it records `stack.set(object, other)`, compares the entries as arrays, and then deletes the `object` entry.

**When two self-referencing values count as equal**
- A cycle counts as equal only when the pairing is the same on both sides. The two objects have to be mapped to each other. For example, `a.self = a` and `b.self = b` match, because the pair `(a, b)` is already on the stack when `a.self` is compared with `b.self`.
- If `a` is already paired with some other object, or only one side is on the stack, the check fails. The cycle shapes then differ, and the result is `false`.
- Hitting a cycle only cuts off that branch. The remaining keys or elements are still compared, so a difference elsewhere still makes the values unequal.
- For Maps and Sets, the check is weaker. It only tests that `object` is paired with `other`.

**Uncertainty:** I read the code but did not run any cyclic examples. The checkout's HEAD SHA check failed because `/dev/null` was blocked, so I did not confirm that the files match commit `2b5e6f7399`. The line numbers come from `lodash.js` in the local checkout, and `~5766` is approximate.