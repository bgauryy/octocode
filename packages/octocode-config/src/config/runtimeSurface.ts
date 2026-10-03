import {
  DEFAULT_RUNTIME_SURFACE,
  type RuntimeSurface,
} from './contract.generated.js';

export type { RuntimeSurface } from './contract.generated.js';
export { RUNTIME_SURFACES } from './contract.generated.js';

let runtimeSurface: RuntimeSurface = DEFAULT_RUNTIME_SURFACE;

export function setRuntimeSurface(surface: RuntimeSurface): void {
  runtimeSurface = surface;
}

export function getRuntimeSurface(): RuntimeSurface {
  return runtimeSurface;
}

export function _resetRuntimeSurface(): void {
  runtimeSurface = DEFAULT_RUNTIME_SURFACE;
}

/**
 * Per-request execution budget for interactive surfaces (CLI and MCP): cold
 * start plus one logical LSP request — initialize, Java readiness, retries,
 * delays, and transport overhead. The native CLI's
 * `INTERACTIVE_EXECUTION_TIMEOUT_SECS` uses the same value.
 */
export const INTERACTIVE_EXECUTION_TIMEOUT_SECS = 300;
