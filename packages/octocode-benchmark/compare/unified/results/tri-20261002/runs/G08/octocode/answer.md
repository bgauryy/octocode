**Root cause:** The state proxy in `packages/svelte/src/internal/client/proxy.js` had no reactive dependency in its `getOwnPropertyDescriptor` trap. `Object.hasOwn(proxy, prop)` goes through that trap. The `has` trap, which `prop in proxy` uses, did create a dependency. So `'x' in s` was reactive, but `Object.hasOwn(s, 'y')` never re-ran when `y` was added or deleted (issue #18837, reported by Rich-Harris).

The diagnosis comes from the PR description and diff. I did not read the pre-fix trap source separately.

**Fix:** PR #18838 was merged on 2026-09-18 and closed the issue. It changes the `getOwnPropertyDescriptor` trap in `proxy.js`, around line 204:
- The trap now starts with `this.has?.(target, prop);`. This reuses the `has` trap's per-property source creation and dependency tracking, including its rules for inherited properties.
- It then looks up the property's source with `sources.get(prop)`. If a source exists, it reads it with `get(s)`.
  - If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
  - Otherwise it sets `descriptor.value` when the real descriptor has a value. If there is no real descriptor, it returns a synthesized `{enumerable: true, configurable: true, ...}` descriptor.
- The old code only read the source in some branches. Now the source is always read, which creates the dependency.

The PR also adds a changeset (`.changeset/reactive-has-own.md`) and a regression test, `tests/runtime-runes/samples/object-has-own-reactive`. The test adds and then deletes `state.y` and checks that `{Object.hasOwn(state,'y')}` flips false → true → false.

**Uncertainty:** I read only the diff, not the merged file, so I can't cite exact line numbers beyond the hunk start at line 204.