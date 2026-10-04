**Short answer:** `_.isEqual` avoids infinite recursion by keeping a `Stack` of the object pairs it is currently comparing. When it meets a pair it is already part of, it returns an answer immediately instead of recursing. Two self-referencing values count as equal only if the cycles pair up consistently: each value's recorded partner must be the other value. All of this is in `/Users/bgaryy/code/octocode/octocode-local-testing/repos/javascript/lodash.js`.

**How the stack gets set up**
- `baseIsEqualDeep` creates `stack || (stack = new Stack)` before it calls `equalArrays`, `equalByTag` or `equalObjects` (lines 3359–3362 and 3379–3380).
- It passes the same stack to every recursive `equalFunc` call.

**Arrays (`equalArrays`, lines 5706–5766)**
1. It looks up both values with `stack.get(array)` and `stack.get(other)` (5715–5716).
2. If both are already on the stack, it stops recursing and returns `arrStacked == other && othStacked == array` (5717–5718).
3. Otherwise it records the pair in both directions with `stack.set(array, other)` and `stack.set(other, array)` (5724–5725).
4. It then compares the elements and recurses through `equalFunc(..., stack)` (5748, 5757).
5. It removes both entries with `stack['delete']` before returning (5763–5764).

**Plain objects (`equalObjects`, lines 5881–5924)**
- The logic is the same, using the same lookup, mutual-pairing check, set and delete (5881–5888, 5923–5924).
- The recursion happens at line 5903.

**Maps and Sets (`equalByTag`, lines 5830–5839)**
- It uses a one-sided check: `stacked = stack.get(object); if (stacked) return stacked == other;` (5830–5832).
- It then sets `stack.set(object, other)`, compares via `equalArrays(convert(object), convert(other), ...)`, and deletes the entry (5837–5839).

**When two cyclic values are equal**
- A back-reference is accepted only if the pairing matches. `object` must already be paired with `other`, and `other` with `object` (5884).
- If the cycles line up, the check returns `true`. That `true` counts as equality for that property or element, so the comparison carries on with the remaining keys.
- If one value cycles back to a different partner than the other value does, the check returns `false`. An example is `a.self = a` compared with a structure whose cycle goes through a different object.
- The stack is path-scoped. Entries are deleted when a comparison finishes, so it only tracks the pairs currently being compared. It is not a global "seen" set.

**Not verified**
- I did not read `equalByTag`'s lines 5771–5877 beyond what the search matches showed. I also did not run any test cases.