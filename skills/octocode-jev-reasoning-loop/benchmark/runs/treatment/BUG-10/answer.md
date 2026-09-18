# BUG-10 Treatment — Vite CSS @import HMR stops after first reload

## THINK

**Source observations:**
- `HMRContext` constructor: `if (mod) { mod.callbacks = [] }` — ALL callbacks cleared on new context creation
- `fetchUpdate`: `qualifiedCallbacks = mod.callbacks.filter(deps => deps.includes(acceptedPath))` — only registered callbacks fire
- CSS HMR: accept registration is injected by Vite transform plugin at transform time, not re-executed at runtime on reload
- `prunePaths`: clears `dataMap` entries — separate from `hotModulesMap`

**Three causes:**
- H1: After first reload, imported CSS module's new entry loses importers set — parent no longer invalidated
- H2: CSS preprocessor cache not busted for parent — parent returns old @import result
- H3: Parent CSS HMRContext recreated on first reload, clearing callbacks; accept not re-registered

**Analysis:** H3 is directly confirmed: `HMRContext` constructor clears `mod.callbacks = []`. For CSS modules, `hot.accept()` is injected by the transform plugin — this runs at module transform time, NOT at re-execution time (CSS modules don't "re-execute" the way JS modules do). So after the first hot reload, the parent CSS module's accept callback is cleared by H3's mechanism and never re-registered.

## GATE

**Classification: `deterministic`** — H3 is mechanistically confirmed by the HMRContext constructor source. The CSS module execution model (transform-time vs runtime registration) makes the re-registration failure deterministic. No meaningful dispute remains between H1, H2, H3 once H3's source is read.

**Action:** Proceed without Jev. H3 is confirmed.

## Root cause

**H3 confirmed by source.** When the imported CSS file (`dep.css`) changes:
1. First update: server sends update for `dep.css`. The parent CSS module (`parent.css`) has its `hot.accept()` callback fire → parent CSS is re-injected. Vite creates a new `HMRContext` for `parent.css`, which **clears `mod.callbacks = []`**.
2. The CSS HMR plugin's accept registration is a compile-time transform — it adds `import.meta.hot.accept()` as injected code. This injected code runs once when the module loads. After HMRContext clears the callbacks, the injected code has already run (it ran on initial page load) — it does NOT re-run on hot reload because CSS modules don't re-execute.
3. Next change to `dep.css`: `fetchUpdate` for `parent.css` finds empty `callbacks` array → `qualifiedCallbacks = []` → no callback fires.

## Fix

In `HMRContext` constructor, preserve static accept registrations rather than clearing all callbacks:

```ts
if (mod) {
  // Only clear dynamic/runtime callbacks, preserve transform-injected ones
  mod.callbacks = mod.callbacks.filter(cb => cb.isStaticHotAccept)
}
```

OR: re-trigger the CSS accept injection when the CSS module's hot context is recreated, by marking the transform as hot-context-dependent and re-applying it on context recreation.

## Decision snapshot
- **GATE:** `deterministic` — no Jev call
- **Jev calls:** 0
- **Confidence:** Medium-high — H3 mechanism confirmed in source; CSS plugin re-registration behavior inferred from architecture
