**Root cause:** The `$state` proxy in `packages/svelte/src/internal/client/proxy.js` only created a reactive dependency in its `has` trap, which backs `prop in s`. `Object.hasOwn(s, prop)` calls the `getOwnPropertyDescriptor` trap instead. That trap read the raw target and only peeked at `source?.v` when the descriptor was undefined, so it never subscribed to the per-property source. Adding or deleting the property therefore didn't invalidate the expression. This is my reading of the pre-fix code in the PR diff. I didn't read the file at the pre-fix commit.

The issue (#18837, opened 2026-09-18 by Rich-Harris) shows `{'x' in s}` updating while `{Object.hasOwn(s, 'y')}` stays stale.

**Fix:** PR #18838, merged 2026-09-18 ("fix: make Object.hasOwn reactive for state proxies"). It changes the `getOwnPropertyDescriptor` trap in `proxy.js` (around line 204):
- The trap now calls `this.has?.(target, prop)` first, so it creates or reads the same per-property source as the `has` trap.
- It then looks up `sources.get(prop)`. If a source exists, it reads it with `get(s)`, which registers the dependency. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`.
- Otherwise it either sets `descriptor.value` to the current value (when the target has a value descriptor) or returns a synthetic `{enumerable: true, configurable: true, ...}` descriptor.

The PR also adds a changeset (`.changeset/reactive-has-own.md`) and a regression test, `tests/runtime-runes/samples/object-has-own-reactive`. The test adds and then deletes `y` and checks that the output flips between `false` and `true`.

**Uncertainty:** I didn't read the full post-fix trap, so I haven't seen the remainder of the returned descriptor. The diff also doesn't show the `has` trap's own code, so I haven't verified that it handles inherited properties the way the PR description says.