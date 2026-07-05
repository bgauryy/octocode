import { ContentSanitizer } from './contentSanitizer.js';
import type { ISanitizer, ToolResult } from './types.js';

const DEFAULT_TOOL_TIMEOUT_MS = 60_000;

export interface SecurityDepsConfig {
  sanitizer?: ISanitizer;

  defaultTimeoutMs?: number;
}

let _deps: SecurityDepsConfig = {};

function getSanitizer(): ISanitizer {
  return _deps.sanitizer ?? ContentSanitizer;
}

function getTimeoutMs(override?: number): number {
  return override ?? _deps.defaultTimeoutMs ?? DEFAULT_TOOL_TIMEOUT_MS;
}

export function configureSecurity(deps: SecurityDepsConfig): void {
  _deps = { ..._deps, ...deps };
}

function createErrorResult(text: string): ToolResult {
  return { content: [{ type: 'text', text }], isError: true };
}

// Combine an external caller-provided AbortSignal with an internal one so that
// aborting either cancels the tool. Returns the single source unchanged when
// only one (or none) is present, preserving existing behavior.
function mergeAbortSignals(
  external: AbortSignal | undefined,
  internal: AbortSignal | undefined
): AbortSignal | undefined {
  if (!external) return internal;
  if (!internal) return external;
  return AbortSignal.any([external, internal]);
}

function withToolTimeout(
  toolName: string,
  promise: Promise<ToolResult>,
  signal?: AbortSignal,
  timeoutMs?: number,
  onTimeout?: () => void
): Promise<ToolResult> {
  const timeout = getTimeoutMs(timeoutMs);

  if (signal?.aborted) {
    return Promise.resolve(
      createErrorResult(`Tool '${toolName}' was cancelled before execution.`)
    );
  }

  return new Promise<ToolResult>(resolve => {
    const timer = setTimeout(() => {
      onTimeout?.();
      resolve(
        createErrorResult(
          `Tool '${toolName}' timed out after ${timeout / 1000}s. Try reducing query complexity or scope.`
        )
      );
    }, timeout);

    const onAbort = () => {
      clearTimeout(timer);
      resolve(
        createErrorResult(`Tool '${toolName}' was cancelled by the client.`)
      );
    };

    signal?.addEventListener('abort', onAbort, { once: true });

    // Re-check after registering the listener: the signal may have been aborted
    // in the window between addEventListener and this line, which the listener
    // alone cannot catch.
    if (signal?.aborted) {
      clearTimeout(timer);
      signal.removeEventListener('abort', onAbort);
      resolve(
        createErrorResult(`Tool '${toolName}' was cancelled before execution.`)
      );
      return;
    }

    promise
      .then(result => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
        resolve(result);
      })
      .catch(error => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
        resolve(
          createErrorResult(
            `Tool '${toolName}' failed: ${error instanceof Error ? error.message : 'Unknown error'}`
          )
        );
      });
  });
}

interface RunSecureOptions<T extends Record<string, unknown>, TAuth> {
  toolName: string;
  handler: (
    sanitizedArgs: T,
    authInfo?: TAuth,
    sessionId?: string
  ) => Promise<ToolResult>;
  args: unknown;
  authInfo?: TAuth;
  sessionId?: string;
  signal?: AbortSignal;
  timeoutMs?: number;
  // Called when the tool times out, before the timeout error resolves. Lets
  // callers abort an internal AbortController to signal the handler to stop
  // without changing every handler's signature. Internal opt-in only; the
  // public withSecurityValidation / withBasicSecurityValidation wrappers do
  // not expose this yet.
  onTimeout?: () => void;
  // Internal abort signal merged (via AbortSignal.any) with the external caller
  // `signal`. Groundwork for callers to pass an internal controller.
  abortSignal?: AbortSignal;
}

async function runSecure<T extends Record<string, unknown>, TAuth>(
  opts: RunSecureOptions<T, TAuth>
): Promise<ToolResult> {
  const {
    toolName,
    handler,
    args,
    authInfo,
    sessionId,
    signal,
    timeoutMs,
    onTimeout,
    abortSignal,
  } = opts;
  try {
    const sanitizer = getSanitizer();
    const validation = sanitizer.validateInputParameters(
      args as Record<string, unknown>
    );
    if (!validation.isValid) {
      return createErrorResult(
        `Security validation failed: ${validation.warnings.join('; ')}`
      );
    }
    const sanitizedParams = validation.sanitizedParams as Record<
      string,
      unknown
    >;
    const mergedSignal = mergeAbortSignals(signal, abortSignal);
    const rawResult = await withToolTimeout(
      toolName,
      handler(sanitizedParams as T, authInfo, sessionId),
      mergedSignal,
      timeoutMs,
      onTimeout
    );
    return rawResult;
  } catch (error) {
      return createErrorResult(
        `Security validation error: ${
          error instanceof Error ? error.message : 'Unknown error'
      }`
    );
  }
}

export function withSecurityValidation<
  T extends Record<string, unknown>,
  TAuth = unknown,
>(
  toolName: string,
  toolHandler: (
    sanitizedArgs: T,
    authInfo?: TAuth,
    sessionId?: string
  ) => Promise<ToolResult>,
  options?: { timeoutMs?: number }
): (
  args: unknown,
  extra: { authInfo?: TAuth; sessionId?: string; signal?: AbortSignal }
) => Promise<ToolResult> {
  return (
    args: unknown,
    {
      authInfo,
      sessionId,
      signal,
    }: { authInfo?: TAuth; sessionId?: string; signal?: AbortSignal } = {}
  ) =>
    runSecure<T, TAuth>({
      toolName,
      handler: toolHandler,
      args,
      authInfo,
      sessionId,
      signal,
      timeoutMs: options?.timeoutMs,
    });
}

export function withBasicSecurityValidation<T extends object>(
  toolHandler: (sanitizedArgs: T) => Promise<ToolResult>,
  toolName?: string,
  options?: { timeoutMs?: number }
): (args: unknown, extra?: { signal?: AbortSignal }) => Promise<ToolResult> {
  const handler = (sanitizedArgs: Record<string, unknown>) =>
    toolHandler(sanitizedArgs as T);
  const effectiveName = toolName ?? 'tool';
  return (args: unknown, extra?: { signal?: AbortSignal }) =>
      runSecure({
        toolName: effectiveName,
        handler,
        args,
        signal: extra?.signal,
        timeoutMs: options?.timeoutMs,
      });
}
