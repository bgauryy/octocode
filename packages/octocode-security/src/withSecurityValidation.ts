import { ContentSanitizer } from './contentSanitizer.js';
import type { ISanitizer, ToolResult } from './types.js';
import {
  extractResearchFields,
  extractRepoOwnerFromParams,
} from './paramExtractors.js';

const SECURITY_VALIDATION_FAILED_CODE = 'TOOL_SECURITY_VALIDATION_FAILED';

const DEFAULT_TOOL_TIMEOUT_MS = 60_000;

export interface SecurityDepsConfig {
  sanitizer?: ISanitizer;

  defaultTimeoutMs?: number;
  logToolCall?: (
    toolName: string,
    repos: string[],
    goal?: string,
    rGoal?: string,
    reasoning?: string
  ) => Promise<void>;
  logSessionError?: (toolName: string, errorCode: string) => Promise<void>;
  isLoggingEnabled?: () => boolean;
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

function withToolTimeout(
  toolName: string,
  promise: Promise<ToolResult>,
  signal?: AbortSignal,
  timeoutMs?: number
): Promise<ToolResult> {
  const timeout = getTimeoutMs(timeoutMs);

  if (signal?.aborted) {
    return Promise.resolve(
      createErrorResult(`Tool '${toolName}' was cancelled before execution.`)
    );
  }

  return new Promise<ToolResult>(resolve => {
    const timer = setTimeout(() => {
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
}

async function runSecure<T extends Record<string, unknown>, TAuth>(
  opts: RunSecureOptions<T, TAuth>
): Promise<ToolResult> {
  const { toolName, handler, args, authInfo, sessionId, signal, timeoutMs } =
    opts;
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
    const rawResult = await withToolTimeout(
      toolName,
      handler(sanitizedParams as T, authInfo, sessionId),
      signal,
      timeoutMs
    );
    if (!rawResult.isError && _deps.isLoggingEnabled?.()) {
      handleBulk(toolName, sanitizedParams);
    }
    return rawResult;
  } catch (error) {
    _deps
      .logSessionError?.(toolName, SECURITY_VALIDATION_FAILED_CODE)
      .catch(() => {});
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

function handleBulk(toolName: string, params: Record<string, unknown>): void {
  const queries = params.queries;
  const items =
    queries && Array.isArray(queries) && queries.length > 0
      ? (queries as Array<Record<string, unknown>>)
      : [params];

  for (const item of items) {
    const repos = extractRepoOwnerFromParams(item);
    const fields = extractResearchFields(item);
    _deps
      .logToolCall?.(
        toolName,
        repos,
        fields.mainResearchGoal,
        fields.researchGoal,
        fields.reasoning
      )
      .catch(() => {});
  }
}
