**Root cause:** The `$state` proxy in `packages/svelte/src/internal/client/proxy.js` made `prop in s` reactive, but `Object.hasOwn(s, prop)` was not. `Object.hasOwn` calls the proxy's `getOwnPropertyDescriptor` trap, and that trap never created or read a per-property source. The `has` trap does, which is why `in` worked. An expression using `Object.hasOwn(s, 'y')` therefore had no dependency on `y`, and adding or deleting `y` didn't re-run it. This comes from the issue's repro and the PR diff. I did not read the pre-fix `proxy.js` in full.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies". It is merged as `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`, and the issue was closed on 2026-09-18. The diff to `proxy.js` makes these changes in the `getOwnPropertyDescriptor` trap, around line 204:
- **Reactive dependency:** the trap now begins with `this.has?.(target, prop);`. That reuses the `has` trap's source creation and eligibility rules, so inherited properties keep their existing behavior.
- **Source handling:** it looks up `sources.get(prop)` once and reads it with `get(s)`, which registers the dependency.
  - If the source's value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
  - If a real descriptor with a `value` exists, it substitutes the source's value.
  - Otherwise it synthesizes an `{enumerable: true, configurable: true, ...}` descriptor.
- **Tests:** a new sample at `packages/svelte/tests/runtime-runes/samples/object-has-own-reactive/` checks that `Object.hasOwn` updates on both add and delete.
- **Changeset:** a patch changeset, `.changeset/reactive-has-own.md`, is included.

**Uncertainty:** the diff output was cut off partway through the synthesized descriptor, so I didn't see its exact remaining fields. I also didn't check out the repo to confirm the final line numbers at the merge commit.