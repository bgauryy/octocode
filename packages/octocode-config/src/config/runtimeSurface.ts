/**
 * Which interface is driving the shared tool core right now.
 *
 * Local tools and cloning default to enabled on both surfaces (see
 * `resolveLocal`). The surface marker remains available to consumers that vary
 * presentation or response handling by interface.
 *
 * `ENABLE_LOCAL=true`/`false` (or `local.enabled`) explicitly enables or
 * disables local tools on every surface, overriding the shared default.
 *
 * Defaults to `mcp`, the primary consumer. The CLI binary calls
 * `setRuntimeSurface('cli')` at startup before tool execution.
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
