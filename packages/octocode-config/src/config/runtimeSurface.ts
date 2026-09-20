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
