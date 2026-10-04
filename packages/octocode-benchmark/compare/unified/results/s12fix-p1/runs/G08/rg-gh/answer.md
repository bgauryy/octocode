**Root cause:** The state proxy in `packages/svelte/src/internal/client/proxy.js` had a `has` trap that created a per-property reactive dependency, so `'y' in s` was reactive. Its `getOwnPropertyDescriptor` trap didn't do that. `Object.hasOwn(s, 'y')` calls `getOwnPropertyDescriptor`, not `has`. Adding or deleting the property therefore never invalidated the expression. I read the pre-fix trap only through the PR diff, not the full file.

**Fix:** PR #18838, "fix: make Object.hasOwn reactive for state proxies". It says "Fixes #18837" and was merged 2026-09-18 as commit `a72dc8eadbf63189d9ae12e9ea18711378f8f12f`. The issue's timeline shows that commit referencing it before it closed.

Changes to the `getOwnPropertyDescriptor` trap in `proxy.js`:
- It now starts with `this.has?.(target, prop);`. This reuses the `has` trap's logic for creating the dependency, so the eligibility rules match `in`.
- It looks up `sources.get(prop)` once. If a source exists, it reads it with `get(s)`, which subscribes to it.
  - If the value is `UNINITIALIZED`, which means the property was deleted, it returns `undefined`.
  - Otherwise it uses the source's value. It overwrites `descriptor.value` if the target has a data descriptor, and returns a synthesized `{enumerable: true, configurable: true, ...}` descriptor if the target has none.

The PR also adds:
- a runtime-runes test, `object-has-own-reactive`, that covers adding and deleting `y`;
- a patch changeset, `.changeset/reactive-has-own.md`.

**Uncertainty:** I didn't view the full post-fix file. The trap's code after the `enumerable: true, configurable: true` lines was cut off in the diff I saw.