import { ContentSanitizer } from './contentSanitizer.js';
import type {
  ISanitizer,
  ToolResult,
  ToolSecurityContext,
} from './types.js';

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

function timeoutReason(toolName: string): DOMException | Error {
  const message = `Tool '${toolName}' timed out`;
  if (typeof DOMException !== 'undefined') {
    return new DOMException(message, 'TimeoutError');
  }
  const error = new Error(message);
  error.name = 'TimeoutError';
  return error;
}

function abortController(controller: AbortController, reason: unknown): void {
  try {
    controller.abort(reason);
  } catch {
    controller.abort();
  }
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
  start: () => Promise<ToolResult>,
  signal?: AbortSignal,
  timeoutMs?: number,
  onTimeout?: () => void,
  timeoutSignal?: AbortSignal
): Promise<ToolResult> {
  const timeout = getTimeoutMs(timeoutMs);

  if (signal?.aborted) {
    return Promise.resolve(
      createErrorResult(`Tool '${toolName}' was cancelled before execution.`)
    );
  }

  return new Promise<ToolResult>(resolve => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout>;
    let onAbort: () => void;
    const finish = (result: ToolResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (onAbort) signal?.removeEventListener('abort', onAbort);
      resolve(result);
    };
    timer = setTimeout(() => {
      onTimeout?.();
      finish(
        createErrorResult(
          `Tool '${toolName}' timed out after ${timeout / 1000}s. Try reducing query complexity or scope.`
        )
      );
    }, timeout);

    onAbort = () => {
      if (timeoutSignal?.aborted) {
        finish(
          createErrorResult(
            `Tool '${toolName}' timed out after ${timeout / 1000}s. Try reducing query complexity or scope.`
          )
        );
        return;
      }
      finish(
        createErrorResult(`Tool '${toolName}' was cancelled by the client.`)
      );
    };

    signal?.addEventListener('abort', onAbort, { once: true });

    // Re-check after registering the listener: the signal may have been aborted
    // in the window between addEventListener and this line, which the listener
    // alone cannot catch.
    if (signal?.aborted) {
      finish(
        createErrorResult(`Tool '${toolName}' was cancelled before execution.`)
      );
      return;
    }

    start()
      .then(result => finish(result))
      .catch(error => {
        finish(
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
    context: ToolSecurityContext<TAuth>
  ) => Promise<ToolResult>;
  args: unknown;
  authInfo?: TAuth;
  sessionId?: string;
  signal?: AbortSignal;
  timeoutMs?: number;
  // Optional hook invoked when the timeout controller aborts the handler.
  onTimeout?: () => void;
  // Optional caller-owned abort signal merged with external caller signal and
  // the wrapper's timeout controller.
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
  const timeoutController = new AbortController();
  const timeoutAbort = () => {
    abortController(timeoutController, timeoutReason(toolName));
    onTimeout?.();
  };
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
    const mergedSignal = mergeAbortSignals(
      mergeAbortSignals(signal, abortSignal),
      timeoutController.signal
    );
    const rawResult = await withToolTimeout(
      toolName,
      () =>
        handler(sanitizedParams as T, {
          authInfo,
          sessionId,
          signal: mergedSignal,
        }),
      mergedSignal,
      timeoutMs,
      timeoutAbort,
      timeoutController.signal
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
    context: ToolSecurityContext<TAuth>
  ) => Promise<ToolResult>,
  options?: { timeoutMs?: number }
): (
  args: unknown,
  extra?: { authInfo?: TAuth; sessionId?: string; signal?: AbortSignal }
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
  toolHandler: (
    sanitizedArgs: T,
    context: ToolSecurityContext<unknown>
  ) => Promise<ToolResult>,
  toolName?: string,
  options?: { timeoutMs?: number }
): (args: unknown, extra?: { signal?: AbortSignal }) => Promise<ToolResult> {
  const handler = (
    sanitizedArgs: Record<string, unknown>,
    context: ToolSecurityContext<unknown>
  ) => toolHandler(sanitizedArgs as T, context);
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
