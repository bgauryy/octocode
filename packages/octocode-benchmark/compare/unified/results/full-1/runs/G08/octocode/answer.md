**Root cause:** `Object.hasOwn(s, prop)` on a `$state` proxy wasn't reactive because the proxy's `getOwnPropertyDescriptor` trap never registered a dependency for a property that doesn't exist yet. `Object.hasOwn` goes through that trap, while the `in` operator goes through the `has` trap. `has` already created a per-property source, so `'x' in s` was reactive. The old `getOwnPropertyDescriptor` trap only called `get(s)` when a source already existed. For a missing property it just peeked at `source?.v`, so adding or deleting the property didn't invalidate the expression.

Issue #18837, "Object.hasOwn(proxy, prop) is not reactive", was opened by Rich-Harris on 2026-09-18. I haven't opened `proxy.js` at a pinned commit. The trap's earlier code and the fix's line changes both come from the PR diff.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies", merged 2026-09-18 and closed the issue. It changes `getOwnPropertyDescriptor` in `packages/svelte/src/internal/client/proxy.js` (hunk at `@@ -204,16 +204,21 @@`, so around line 204 in the pre-fix file):
- The trap now starts with `this.has?.(target, prop);`. That reuses the `has` trap's logic, which creates or tracks the per-property source under the same eligibility rules, so inherited properties keep their existing behavior.
- It then looks up `sources.get(prop)`. If a source exists, it reads it with `get(s)`, which creates the dependency. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`. Otherwise it sets `descriptor.value`, or returns a synthetic `{enumerable: true, configurable: true, ...}` descriptor when the target has none.

The PR also adds:
- A changeset, `.changeset/reactive-has-own.md`.
- A regression test, `packages/svelte/tests/runtime-runes/samples/object-has-own-reactive/`, which adds and deletes `y` and checks that `Object.hasOwn(state, 'y')` toggles between `true` and `false`.

**Uncertainty:** The PR body says the trap "uses the same eligibility constraints as the existing `has` trap." I confirmed that only from the `this.has?.()` call in the diff, not by reading the `has` trap's source. I didn't run the tests. The test counts come from the PR description.