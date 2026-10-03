**Root cause:** `Object.hasOwn(proxy, prop)` wasn't reactive because it goes through the proxy's `getOwnPropertyDescriptor` trap. That trap never registered a dependency for a property that was absent or deleted. The `in` operator goes through the `has` trap, which does register one, so `'x' in s` updated and `Object.hasOwn(s, 'y')` didn't. Issue #18837 ("Object.hasOwn(proxy, prop) is not reactive") was a small repro: adding `s.y` didn't update `{Object.hasOwn(s, 'y')}`.

I read the trap's old code only from the diff hunk, not from the full file. From that hunk, the old trap called `get(s)` only when a source already existed and held a value. When the property didn't exist yet, it didn't create or subscribe to a source. I'm inferring that this is why nothing tracked the later add or delete.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies", merged 2026-09-18 as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`. The change is in `packages/svelte/src/internal/client/proxy.js`, in the `getOwnPropertyDescriptor` trap, at about line 204 in the diff:
- The trap now starts with `this.has?.(target, prop);`. This reuses the `has` trap's logic for creating a per-property source, so a dependency exists even for absent properties. The PR says it uses the same eligibility constraints as `has`, so inherited properties keep their existing behavior.
- The trap then reads `sources.get(prop)` and calls `get(s)` whenever a source exists. That read is what subscribes to the property.
- If the source's value is `UNINITIALIZED` (the property was deleted), the trap returns `undefined`.
- Otherwise it returns the real descriptor with its value replaced, or a synthesized `{enumerable: true, configurable: true, …}` descriptor.

The PR also adds a patch changeset (`.changeset/reactive-has-own.md`) and a regression test, `tests/runtime-runes/samples/object-has-own-reactive/`. The test covers adding and deleting the property in both DOM and hydration modes.

**Uncertainty:** I saw only the `proxy.js` diff hunk, not the whole file. The hunk cuts off partway through the synthesized descriptor, so I haven't seen the rest of it. The line number is from the hunk header, not from reading the file at that commit.