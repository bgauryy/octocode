import {
  createAwarenessClient,
  executeAwarenessCommand,
  type AwarenessCommandCall,
  type AwarenessCommandContext,
  type AwarenessCommandResult,
  type AwarenessExecutableCall,
} from '@octocodeai/octocode-awareness';

export const DEFAULT_AWARENESS_COMMAND_TIMEOUT_MS = 120_000;
export interface AwarenessCommandRunnerOptions extends AwarenessCommandContext {
  timeoutMs?: number;
  sessionId?: string;
}
export type AwarenessRunnerRequest = AwarenessCommandCall | AwarenessExecutableCall;
export type AwarenessCommandRunner = (request: AwarenessRunnerRequest, options: AwarenessCommandRunnerOptions) => Promise<AwarenessCommandResult>;

/** Import the package API directly. Deadlines are cooperative, including lock waits. */
export const runAwarenessCommand: AwarenessCommandRunner = async (request, options) => {
  const timeout = AbortSignal.timeout(options.timeoutMs ?? DEFAULT_AWARENESS_COMMAND_TIMEOUT_MS);
  const signal = options.signal ? AbortSignal.any([options.signal, timeout]) : timeout;
  const { timeoutMs: _timeoutMs, sessionId, ...context } = options;
  if ('operation' in request) {
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
  }
  return executeAwarenessCommand(request, { ...context, signal });
};
