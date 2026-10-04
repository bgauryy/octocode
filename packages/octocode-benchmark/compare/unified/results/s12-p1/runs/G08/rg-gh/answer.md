**Root cause:** the `$state` proxy's `getOwnPropertyDescriptor` trap in `packages/svelte/src/internal/client/proxy.js` didn't create a reactive dependency for a property that didn't exist yet. `Object.hasOwn(proxy, prop)` goes through that trap. The `has` trap, which backs `prop in proxy`, did track existence, so `'x' in s` was reactive and `Object.hasOwn(s, 'x')` wasn't.

The old trap only read an existing property's source, in two cases:
- If the target had a real descriptor with a `value`, it did `get(s)`.
- If the descriptor was `undefined`, it read `source?.v` directly, which doesn't subscribe.

So when the property was absent or later added or deleted, nothing was tracked and the expression never re-ran. I inferred this from the diff; the issue and PR text don't state the cause.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies". It was merged on 2026-09-18 as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`, and that closed #18837. The change to `getOwnPropertyDescriptor` in `proxy.js` (around line 204):
- It now calls `this.has?.(target, prop)` first. That reuses the `has` trap's source creation and eligibility rules, so inherited properties keep their existing behaviour.
- It then reads the source with `get(s)`, which subscribes the caller. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
- Otherwise it returns the descriptor with the live value. If the target has no real descriptor, it returns a synthesized `{enumerable: true, configurable: true, ...}` one.

The PR also adds a regression test, `tests/runtime-runes/samples/object-has-own-reactive`, which checks that adding and then deleting `y` updates `{Object.hasOwn(state,'y')}`. It also adds a patch changeset.

**Uncertainty:** I only read the PR diff and description. I didn't check out the code or run the tests. The PR body reports that the tests pass.