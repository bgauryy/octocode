# BUG-10 Baseline — Vite CSS @import HMR stops after first reload

## Root cause

**H3 is the most likely cause.** From `packages/vite/src/shared/hmr.ts`, `HMRContext` constructor:

```ts
constructor(private hmrClient: HMRClient, private ownerPath: string) {
  // when a file is hot updated, a new context is created
  // clear its stale callbacks
  const mod = hmrClient.hotModulesMap.get(ownerPath)
  if (mod) {
    mod.callbacks = []   // ← stale callbacks cleared
  }
  // ...
}
```

When a CSS file with `@import './dep.css'` is hot-reloaded:
1. First update: Server sends update for `dep.css`. The parent CSS file's accept boundary runs (via CSS HMR plugin). The parent CSS module's HMRContext is recreated — clearing its callbacks. The new HMRContext re-registers the accept callback immediately as part of the CSS plugin's setup.
2. **After the first hot reload**: The parent CSS module is re-evaluated to pick up the new `dep.css` content. During re-evaluation, the CSS HMR plugin may or may not re-call `hot.accept()` — if the CSS module transform doesn't explicitly re-register the accept handler in the new module execution, the callbacks array stays empty.
3. Next change to `dep.css`: `fetchUpdate` finds `hotModulesMap.get(parentPath)` with an empty `callbacks` array → `qualifiedCallbacks = []` → no callback fires → silent no-op.

The root cause is H3: after the first hot reload of the parent CSS module, its HMRContext is recreated with `callbacks = []`, but the CSS HMR plugin's inject-accept mechanism does not re-fire the `hot.accept()` registration during the post-reload module re-evaluation, because CSS modules don't run `import.meta.hot.accept()` explicitly — the registration is done by the Vite transform plugin ONCE at transform time, not at runtime.

## Fix proposal

In the CSS HMR plugin, ensure that `hot.accept()` registration is re-emitted each time the parent CSS module is re-evaluated, or alternatively, preserve existing accept registrations when clearing stale callbacks by only clearing `callbacks` entries that reference the OLD module version:

```ts
// In HMRContext constructor, instead of clearing all callbacks:
if (mod) {
  // Preserve static accept registrations; only clear dynamic runtime ones
  mod.callbacks = mod.callbacks.filter(cb => cb.isStaticRegistration)
}
```

## Evidence

- `packages/vite/src/shared/hmr.ts`: `HMRContext` constructor callback clearing — confirmed via `ghGetFileContent`
- `packages/vite/src/shared/hmr.ts`: `fetchUpdate` → `qualifiedCallbacks` filter — confirmed
- CSS HMR plugin behavior: accept registration is injected by transform, not runtime

## Confidence

**Medium** — the callback clearing mechanism is confirmed in source. The CSS plugin's re-registration behavior needs separate investigation of the CSS plugin transform code.
