/**
 * Which interface is driving the shared tool core right now.
 *
 * Some config defaults differ by surface (see `resolveLocal`):
 *   - `cli`: local tools default to ENABLED; clone defaults to ENABLED.
 *   - `mcp`: local tools default to ENABLED; clone defaults to DISABLED.
 *
 * `ENABLE_LOCAL=false` or `local.enabled=false` explicitly disables local tools
 * on every surface.
 *
 * Defaults to `mcp`, the primary consumer. The CLI binary calls
 * `setRuntimeSurface('cli')` at startup before any tool runs.
 *
 * State lives on `globalThis` (not a module-level variable) so a single shared
 * value is seen even when bundlers (esbuild) inline this module more than once
 * across different package subpath entry points (`/config`, `/direct`, …).
 */
export type RuntimeSurface = 'cli' | 'mcp';

const SURFACE_KEY = '__octocodeRuntimeSurface__';

type SurfaceHolder = { [SURFACE_KEY]?: RuntimeSurface };

export function setRuntimeSurface(surface: RuntimeSurface): void {
  (globalThis as SurfaceHolder)[SURFACE_KEY] = surface;
}

export function getRuntimeSurface(): RuntimeSurface {
  return (globalThis as SurfaceHolder)[SURFACE_KEY] ?? 'mcp';
}

/** Test helper: restore the default surface. */
export function _resetRuntimeSurface(): void {
  delete (globalThis as SurfaceHolder)[SURFACE_KEY];
}
