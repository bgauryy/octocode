**Root cause:** The state proxy's `getOwnPropertyDescriptor` trap in `packages/svelte/src/internal/client/proxy.js` didn't create a reactive dependency for a property that doesn't exist yet. `Object.hasOwn(proxy, prop)` goes through that trap. The `has` trap, which `'x' in s` uses, does subscribe. So `prop in s` was reactive and `Object.hasOwn(s, prop)` wasn't (issue #18837, reported by Rich-Harris).

Before the fix, the trap only read a property's source when the target already had a descriptor with a `value`. When the property was absent, it peeked at `source?.v` directly. That is a plain read, not `get(s)`, so it tracked nothing. Adding or deleting the property therefore never invalidated the `{Object.hasOwn(s, 'y')}` expression.

**Fix:** PR #18838 (merged 2026-09-18, closes #18837) changed the trap in `proxy.js`, with a patch hunk starting around line 204:
- It now calls `this.has?.(target, prop)` first. This reuses the `has` trap's logic for creating a per-property source and subscribing, so inherited properties keep their existing behavior.
- It looks up `sources.get(prop)` once. If a source exists, it reads it with `get(s)`, which tracks it.
  - If the value is `UNINITIALIZED`, the trap returns `undefined`, meaning the property doesn't exist.
  - If a descriptor with a `value` exists, `descriptor.value` is set to the tracked value.
  - Otherwise it returns the synthesized `{enumerable: true, configurable: true, ...}` descriptor. I only saw the first two lines of that descriptor in the diff.

The PR also adds a patch changeset (`.changeset/reactive-has-own.md`) and a runtime-runes test (`object-has-own-reactive`). The test adds and then deletes `state.y` and checks that `Object.hasOwn(state, 'y')` updates in the DOM.

**Uncertainty:** I read the PR diff and description, not the merged file. The `has` trap itself, which the fix reuses, is a claim from the PR text and a call in the diff. I didn't read its code.