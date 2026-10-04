**Root cause:** `Object.hasOwn(proxy, prop)` on a `$state` proxy goes through the proxy's `getOwnPropertyDescriptor` trap. That trap didn't create a reactive dependency on a missing property. The `'x' in s` operator goes through the `has` trap, which does. So when a property was added or deleted, nothing invalidated the `Object.hasOwn` expression.

**Fix:** PR #18838 ("fix: make Object.hasOwn reactive for state proxies"). It says "Fixes #18837" and was merged as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`. I read the diff through the PR files API, not a local checkout, so I have no line numbers.

In `packages/svelte/src/internal/client/proxy.js`, the `getOwnPropertyDescriptor` trap changed in these ways:
- It now calls `this.has?.(target, prop)` first. This subscribes to the per-property source, with the same eligibility rules as the `has` trap, so inherited properties keep their existing behavior.
- It looks up `sources.get(prop)` once and reads it with `get(s)`. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
- Otherwise it overrides `descriptor.value` with the source's value, or synthesizes an enumerable, configurable descriptor when the target has no own descriptor.

The PR also adds:
- A regression test, `tests/runtime-runes/samples/object-has-own-reactive`, which checks that adding and then deleting the property toggles the output between `true` and `false`.
- A patch changeset, `.changeset/reactive-has-own.md`.

**Uncertainty:** I didn't run the tests myself. The test results come from the PR description. I also didn't see the full final trap code, only the diff hunk, so the end of the synthesized-descriptor branch is cut off.