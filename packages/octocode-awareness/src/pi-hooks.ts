/** Public compatibility barrel for pi-hooks.ts. */
export { extractPiWriteTargetPaths, getPiAwarenessSessionId, getPiAwarenessAgentId } from './pi-hooks-inputs.js';
export { evaluateHarnessGuard } from './pi-hooks-guard.js';
export { createPiAwarenessBridge } from './pi-hooks-bridge.js';
export { wirePiAwarenessHooks } from './pi-hooks-wire.js';
export type { PiLikeSessionManager, PiLikeUi, PiLikeContext, PiLikeApi, PiToolEvent, PiAwarenessBridgeOptions } from './pi-hooks-inputs.js';
