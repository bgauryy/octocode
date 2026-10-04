**Short answer:** `_.isEqual` carries a `Stack` through the recursion. The stack is a map that records which `other` value each in-progress `object` is being compared against, and the reverse. If a pair comes up again while it is still being compared, the recursion stops and the pair is judged by the stack's recorded pairing. Two self-referencing values are equal only if the cycles line up: each value must point back to the exact counterpart it is currently paired with.

**Mechanism**
- `baseIsEqualDeep` creates the stack lazily with `stack || (stack = new Stack)` (`lodash.js:3359`, `3372`, `3379`). It passes the stack to `equalArrays`, `equalByTag` and `equalObjects` (`3361-3380`).
- `equalArrays` (`5706-5766`):
  - It looks up both values with `stack.get(array)` and `stack.get(other)` (`5715-5716`).
  - If both are already on the stack, it returns `arrStacked == other && othStacked == array` (`5717-5718`). It does not recurse further.
  - Otherwise it records the pairing both ways with `stack.set(array, other)` and `stack.set(other, array)` (`5724-5725`). The stack is passed into the recursive `equalFunc` calls (`5748`, `5757`).
  - It removes both entries with `stack['delete']` before returning (`5763-5764`).
- `equalObjects` follows the same pattern (`5881-5888` for the check and the `set` calls, `5923-5924` for the `delete` calls). The recursion uses `equalFunc(..., stack)` at `5903`.
- Maps and Sets, in `equalByTag`, use the check "Assume cyclic values are equal" (`5829-5833`): `stacked = stack.get(object); if (stacked) return stacked == other;`. Otherwise they call `stack.set(object, other)`, compare through `equalArrays`, and then `stack['delete'](object)` (`5837-5839`).

**When two cyclic values count as equal**
- **Both values are already on the stack (arrays, objects).** The comparison returns true only if `object` is mapped to `other` and `other` is mapped back to `object` (`5718`, `5884`). That is the test that the two cycles have the same shape.
- **A back-reference to a pair already being compared.** The cycle check returns true without comparing it again. Every other property or element is still compared normally, so the pair is equal only if all of those match too.
- **Mismatched cycles.** For example, `a` refers back to itself while `b` refers to a different object. The stored pairings don't match, so the result is false.
- **Maps and Sets.** Only `object` is looked up. They return `stacked == other` (`5832`), so they never check the reverse pairing.
- **Entries are cleared after the comparison.** The `delete` calls remove the pair once its comparison finishes, so the stack tracks only the pairs currently in progress, not everything visited.

**Uncertainty:** I read only the lines cited above. I did not read lines `5767-5825` or `5843-5880` of `equalByTag`, and I did not run any code.