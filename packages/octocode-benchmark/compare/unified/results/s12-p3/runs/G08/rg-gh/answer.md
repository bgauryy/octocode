**Root cause:** The `$state` proxy's `getOwnPropertyDescriptor` trap in `packages/svelte/src/internal/client/proxy.js` never subscribed to any per-property signal when the property didn't exist yet. `Object.hasOwn(proxy, prop)` goes through that trap. The `in` operator goes through the `has` trap, which does create the dependency. So `'x' in s` was reactive and `Object.hasOwn(s, 'y')` was not. Adding or deleting `y` therefore never invalidated the expression (issue #18837).

**Fix:** PR #18838 ("fix: make Object.hasOwn reactive for state proxies") was merged as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`. It changed the `getOwnPropertyDescriptor` trap in `proxy.js` in three ways:
- The trap now starts with `this.has?.(target, prop);`. This reuses the `has` trap's source creation and eligibility rules, so ownership changes now create a dependency, and inherited properties keep their existing behavior.
- It looks up `sources.get(prop)` once and reads it with `get(s)`, so the read is tracked. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
- Otherwise it patches `descriptor.value` when a descriptor exists. If there is no real descriptor, it returns a synthetic `{enumerable: true, configurable: true, …}` descriptor.

The PR also adds a changeset (`.changeset/reactive-has-own.md`) and a runtime-runes regression test (`tests/runtime-runes/samples/object-has-own-reactive`). The test covers both adding and deleting `y`.

**Uncertainty:** I read the diff through the PR API only and didn't check out the code. The diff is truncated in the middle of the synthetic descriptor, so I haven't seen the rest of that return statement.