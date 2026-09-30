**Root cause:** `Object.hasOwn(proxy, prop)` wasn't reactive because it goes through the proxy's `getOwnPropertyDescriptor` trap, and that trap never subscribed to anything. Issue #18837 reports that `'x' in s` updates but `Object.hasOwn(s, 'y')` doesn't. The `in` operator hits the `has` trap, which creates a per-property source and so tracks the property. The old `getOwnPropertyDescriptor` trap only read from `sources` if a source already existed. If none existed, for example for a property not yet added, it created no dependency. Adding or deleting the property therefore never invalidated the expression.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies". It was merged 2026-09-18 as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f` and says "Fixes #18837". The change is in `packages/svelte/src/internal/client/proxy.js`, in the `getOwnPropertyDescriptor` trap:
- The trap now starts with `this.has?.(target, prop);`. That reuses the `has` trap's source creation and eligibility rules, so it also creates the per-property dependency. Inherited properties keep their existing semantics.
- The trap now reads the source with `get(s)`, which registers the dependency. If the value is `UNINITIALIZED` (the property was deleted), it returns `undefined`. Otherwise it patches `descriptor.value` or synthesizes an `{enumerable: true, configurable: true, ...}` descriptor.
- The old code only read `source.v` directly in the branch where no real descriptor existed. That read didn't register a dependency.

**Tests and changeset:**
- The regression sample is `packages/svelte/tests/runtime-runes/samples/object-has-own-reactive/`. It adds and then deletes `y` and checks that the output flips between `false`, `true` and `false`.
- The PR also adds the changeset `.changeset/reactive-has-own.md`, a patch bump for `svelte`.

**Uncertainty:** I read the diff through `gh pr diff`, not a checkout at the merge commit, so I can't give line numbers in the post-merge file. The `has` trap's implementation wasn't in the diff, so I'm relying on the PR description for how it handles eligibility. I didn't run the tests myself.