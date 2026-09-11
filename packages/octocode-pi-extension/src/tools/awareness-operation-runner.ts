import {
  createAwarenessClient,
  type AwarenessClientContext,
  type AwarenessExecutableCall,
  type AwarenessOperationResult,
} from '@octocodeai/octocode-awareness';

export const DEFAULT_AWARENESS_OPERATION_TIMEOUT_MS = 120_000;

export interface AwarenessOperationRunnerOptions extends AwarenessClientContext {
  timeoutMs?: number;
}

export type AwarenessOperationRunner = (
  request: AwarenessExecutableCall,
  options: AwarenessOperationRunnerOptions,
) => Promise<AwarenessOperationResult>;

/** Execute one canonical operation through the host-bound client. */
export const runAwarenessOperation: AwarenessOperationRunner = async (request, options) => {
  const timeout = AbortSignal.timeout(options.timeoutMs ?? DEFAULT_AWARENESS_OPERATION_TIMEOUT_MS);
  const signal = options.signal ? AbortSignal.any([options.signal, timeout]) : timeout;
  const { timeoutMs: _timeoutMs, sessionId, ...context } = options;
  if (!context.workspace || !context.agentId) {
    return { exitCode: 1, payload: { ok: false, error: 'Awareness operation requires bound workspace and agent identity' } };
  }
  return createAwarenessClient({
    ...context,
    workspace: context.workspace,
    agentId: context.agentId,
    ...(sessionId ? { sessionId } : {}),
    signal,
  }).execute(request);
};
