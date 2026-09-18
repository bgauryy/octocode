# BUG-08 Baseline — Vite HMR accept callback skipped for dynamic imports

## Root cause

**H1 is correct.** From `packages/vite/src/shared/hmr.ts`, `HMRClient.fetchUpdate`:

```ts
private async fetchUpdate(update: Update): Promise<(() => void) | undefined> {
  const { path, acceptedPath, firstInvalidatedBy } = update
  const mod = this.hotModulesMap.get(path)
  if (!mod) {
    // In a code-splitting project,
    // it is common that the hot-updating module is not loaded yet.
    return
  }
  // ...
  const qualifiedCallbacks = mod.callbacks.filter(({ deps }) =>
    deps.includes(acceptedPath),
  )
```

The silent `return` when `hotModulesMap.get(path)` is undefined is the failure mode. The `path` in the HMR update payload comes from the server's module graph, where the module ID is the server-normalized URL. When the **same** module is registered via `import.meta.hot.accept()` after being loaded via `dynamic import()`, its `ownerPath` in `HMRContext` is the URL the client receives from the module runner's import resolution.

The mismatch occurs because:
1. Static imports: the module URL is resolved at transform time and the HMRContext `ownerPath` is set before the page loads → matches exactly what the server sends in `update.path`
2. Dynamic imports via `import()`: the URL is resolved at runtime by the module runner. If there's any difference in the base URL, query parameters, or normalization (e.g., `/src/module.ts` vs `/@fs/path/to/module.ts`), `hotModulesMap.get(path)` returns `undefined` and the update is silently dropped

From `HMRContext.acceptDeps`:
```ts
private acceptDeps(deps: string[], callback: HotCallback['fn'] = () => {}): void {
  const mod: HotModule = this.hmrClient.hotModulesMap.get(this.ownerPath) || {
    id: this.ownerPath,
    callbacks: [],
  }
  mod.callbacks.push({ deps, fn: callback })
  this.hmrClient.hotModulesMap.set(this.ownerPath, mod)
}
```

The key used to register is `this.ownerPath` (the client-side URL). If the server sends a different normalized form in `update.path`, `hotModulesMap.get(update.path)` misses.

## Fix proposal

Normalize the module URL consistently between the HMRContext `ownerPath` and the server-sent `update.path` before the `hotModulesMap.get()` lookup:

```ts
const normalizedPath = normalizeUrl(path)  // strip query, normalize base
const mod = this.hotModulesMap.get(normalizedPath)
  ?? this.hotModulesMap.get(path)  // fallback to exact
```

And ensure `HMRContext` constructor receives a consistently normalized `ownerPath` from the module runner runtime, matching the server's module graph ID format.

## Evidence

- `packages/vite/src/shared/hmr.ts`: `fetchUpdate` with silent return — confirmed via `ghGetFileContent`
- `packages/vite/src/shared/hmr.ts`: `acceptDeps` — `ownerPath` used as hotModulesMap key — confirmed
- `packages/vite/src/shared/hmrHandler.ts`: queue serialization — confirmed separate from the lookup issue

## Confidence

**High** — the lookup + silent return is directly in source. URL normalization mismatch is the standard root cause for this class of HMR miss in Vite.
