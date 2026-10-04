**Root cause:** The state proxy's `getOwnPropertyDescriptor` trap didn't register any reactive dependency. `Object.hasOwn(s, prop)` calls `[[GetOwnProperty]]`, which goes through that trap. The `has` trap, which backs `prop in s`, does read the per-property source signals, so `'x' in s` was reactive. `Object.hasOwn` read nothing, so adding or deleting a key never re-ran the template expression. The issue text doesn't state this explanation. I inferred it from the code the fix changed (`packages/svelte/src/internal/client/proxy.js`, in the `getOwnPropertyDescriptor` trap). I did not read the full file.

**Fix:** PR #18838, merged 2026-09-18. It closed the issue, and the changeset says "make Object.hasOwn reactive for state proxy ownership changes". The changes are in the `getOwnPropertyDescriptor` trap in `proxy.js`:
- The trap now starts with `this.has?.(target, prop);`, so it subscribes to the same tracking as the `in` operator.
- It looks up `sources.get(prop)` once. If a source exists, the trap reads it with `get(s)`.
  - If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
  - Otherwise, if the real target has a data descriptor, it sets `descriptor.value` to the source's value.
  - If the target has no descriptor, it returns a synthetic `{enumerable: true, configurable: true, ...}` descriptor.
- The old code only read the source in a few branches, and not for the deleted case.

The PR also adds a runtime test, `tests/runtime-runes/samples/object-has-own-reactive/`. It checks that `Object.hasOwn(state, 'y')` goes false → true after adding `y`, then back to false after `delete state.y`.

**Uncertainty:** I read only the PR diff, not the surrounding `proxy.js` or the `has` trap. The last lines of the diff hunk are cut off, so I can't confirm the exact shape of the synthetic descriptor's remaining fields.