**Short answer:** `_.isEqual` keeps a `Stack`, which is a map of the objects currently being compared. Each object is paired with the object it is being compared to. If the comparison meets a pair that is already in progress, it stops recursing and returns a result straight from the map. Two self-referencing values are equal only when the cycles line up. Every point where one value revisits an object must be matched by the other value revisiting its counterpart.

**Mechanism** (all in `lodash.js`):
- `baseIsEqualDeep` creates the stack lazily with `stack || (stack = new Stack)` and passes it down (`lodash.js:3359`, `3372`, `3379`). The `equalArrays` call is at `lodash.js:3359-3362`.
- `equalArrays` (`lodash.js:5706`):
  - It looks up both `array` and `other` in the stack (`5715-5716`).
  - If both are already there, it returns `arrStacked == other && othStacked == array` (`5717-5719`).
  - Otherwise it records the pair in both directions with `stack.set(array, other)` and `stack.set(other, array)` (`5724-5725`).
  - It removes both entries with `stack['delete']` when it finishes (`5763-5764`).
- `equalObjects` (`lodash.js:5863`) does the same. It looks up both objects at `5881-5882` and returns `objStacked == other && othStacked == object` at `5883-5884`. It sets both entries at `5887-5888` and deletes them at `5923-5924`. I did not print `5883-5884`, but the check follows the same pattern as in `equalArrays`.
- `equalByTag` handles Map and Set (`lodash.js:5830-5839`). It calls `stack.get(object)` and, if there is an entry, returns `stacked == other`. Otherwise it sets `object → other`, compares the converted entries with `equalArrays`, and deletes the entry. This path stores one direction only, and the code comment reads "Assume cyclic values are equal."

**When two cyclic values count as equal:**
- A revisited pair is equal if `object` is already mapped to `other` and `other` is already mapped to `object`. It is not equal if either one is mapped to something else.
- The check is not "have I seen this before?". It asks whether each side is paired with the same counterpart as before.
- So `a = {x: a}` and `b = {x: b}` are equal. A cycle compared against a different structure fails the pairing check and returns `false`.
- Because entries are deleted on exit, the stack only holds the current path. A shared reference that is not a cycle is compared normally.

**Not verified:** I did not run any examples. The `a`/`b` case above comes from reading the code. The `git rev-parse HEAD` command failed here because of a sandbox error, so I could not confirm that the checkout is at commit `2b5e6f7399`. The line numbers come from the local checkout, which you said is at that commit.