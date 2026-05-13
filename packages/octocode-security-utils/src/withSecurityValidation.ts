import { ContentSanitizer } from './contentSanitizer.js';
import type { ISanitizer, ToolResult } from './types.js';
import {
  extractResearchFields,
  extractRepoOwnerFromParams,
} from './paramExtractors.js';

/** Error code for security validation failures */
const SECURITY_VALIDATION_FAILED_CODE = 'TOOL_SECURITY_VALIDATION_FAILED';

/**
 * Default timeout for tool execution (1 minute).
 *
 * Timeout interaction: This is the OUTER timeout — it applies to the entire tool
 * invocation. Bulk tools also use a per-query timeout. For multi-query operations,
 * the outer timeout dominates: e.g. 3 queries at 55s each would exceed 60s total,
 * so the outer timeout fires before all complete.
 */
const DEFAULT_TOOL_TIMEOUT_MS = 60_000;

/**
 * Dependency injection interface for octocode-security-utils.
 * Call configureSecurity() once at application startup to inject these deps.
 */
export interface SecurityDepsConfig {
  /** Custom sanitizer implementation (defaults to ContentSanitizer). */
  sanitizer?: ISanitizer;
  /** Default timeout for all tool invocations in ms (default: 60000). */
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
  isLocalTool?: (name: string) => boolean;
}

let _deps: SecurityDepsConfig = {};

function getSanitizer(): ISanitizer {
  return _deps.sanitizer ?? ContentSanitizer;
}

function getTimeoutMs(override?: number): number {
  return override ?? _deps.defaultTimeoutMs ?? DEFAULT_TOOL_TIMEOUT_MS;
}

/**
 * Configure security module dependencies.
 * Call once at application startup to inject logging and tool-name resolution.
 *
 * @example
 * ```ts
 * configureSecurity({
 *   logToolCall: async (name, repos) => console.log(`[${name}] ${repos}`),
 *   isLoggingEnabled: () => true,
 *   isLocalTool: (name) => name.startsWith('local'),
 * });
 * ```
 */
export function configureSecurity(deps: SecurityDepsConfig): void {
  _deps = { ..._deps, ...deps };
}

/** Creates a simple error result for security failures */
function createErrorResult(text: string): ToolResult {
  return { content: [{ type: 'text', text }], isError: true };
}

/**
 * Wraps a promise with a timeout that respects an optional AbortSignal.
 * Returns an error result instead of throwing on timeout.
 */
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

/**
 * Security wrapper for tools that require authentication.
 *
 * Use this wrapper for tools that:
 * - Need `authInfo` or `sessionId` passed to the handler
 * - Should log queries to session telemetry via `handleBulk`
 * - Access remote APIs (GitHub, GitLab, NPM, etc.)
 *
 * Provides: input sanitization, 60s timeout, auth passthrough, session logging.
 *
 * `TAuth` is the auth-info type from your framework (e.g. MCP's `AuthInfo`).
 * It defaults to `unknown` so no framework dependency is required.
 *
 * @see withBasicSecurityValidation for local tools that don't need auth
 *
 * @example
 * ```ts
 * const searchCode = withSecurityValidation<{ query: string }>(
 *   'github_search_code',
 *   async (args, authInfo) => {
 *     const results = await api.search(args.query);
 *     return { content: [{ type: 'text', text: JSON.stringify(results) }] };
 *   }
 * );
 * ```
 */
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
  const toolTimeoutMs = options?.timeoutMs;
  return async (
    args: unknown,
    {
      authInfo,
      sessionId,
      signal,
    }: { authInfo?: TAuth; sessionId?: string; signal?: AbortSignal } = {}
  ): Promise<ToolResult> => {
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
      if (_deps.isLoggingEnabled?.()) {
        handleBulk(toolName, sanitizedParams);
      }
      const rawResult = await withToolTimeout(
        toolName,
        toolHandler(validation.sanitizedParams as T, authInfo, sessionId),
        signal,
        toolTimeoutMs
      );
      return rawResult;
    } catch (error) {
      _deps
        .logSessionError?.(toolName, SECURITY_VALIDATION_FAILED_CODE)
        .catch(() => {});

      return createErrorResult(
        `Security validation error: ${error instanceof Error ? error.message : 'Unknown error'}`
      );
    }
  };
}

/**
 * Lightweight security wrapper for local filesystem and LSP tools.
 *
 * Use this wrapper for tools that:
 * - Operate on local files only (no remote API access)
 * - Don't need `authInfo` or `sessionId`
 * - Don't need session telemetry logging
 *
 * Provides: input sanitization, 60s timeout.
 * Does NOT provide: auth passthrough, session logging.
 *
 * @see withSecurityValidation for remote tools that need auth + logging
 *
 * @example
 * ```ts
 * const readFile = withBasicSecurityValidation<{ path: string }>(
 *   async (args) => {
 *     const content = await fs.promises.readFile(args.path, 'utf-8');
 *     return { content: [{ type: 'text', text: content }] };
 *   },
 *   'local_read_file'
 * );
 * ```
 */
export function withBasicSecurityValidation<T extends object>(
  toolHandler: (sanitizedArgs: T) => Promise<ToolResult>,
  toolName?: string,
  options?: { timeoutMs?: number }
): (args: unknown, extra?: { signal?: AbortSignal }) => Promise<ToolResult> {
  const toolTimeoutMs = options?.timeoutMs;
  return async (
    args: unknown,
    extra?: { signal?: AbortSignal }
  ): Promise<ToolResult> => {
    const signal = extra?.signal;
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

      if (
        toolName &&
        _deps.isLocalTool?.(toolName) &&
        _deps.isLoggingEnabled?.() &&
        validation.sanitizedParams &&
        typeof validation.sanitizedParams === 'object'
      ) {
        handleBulk(
          toolName,
          validation.sanitizedParams as Record<string, unknown>
        );
      }

      const rawResult = await withToolTimeout(
        toolName || 'tool',
        toolHandler(validation.sanitizedParams as T),
        signal,
        toolTimeoutMs
      );
      return rawResult;
    } catch (error) {
      _deps
        .logSessionError?.(
          toolName || 'basic_security_validation',
          SECURITY_VALIDATION_FAILED_CODE
        )
        .catch(() => {});

      return createErrorResult(
        `Security validation error: ${error instanceof Error ? error.message : 'Unknown error'}`
      );
    }
  };
}

function handleBulk(toolName: string, params: Record<string, unknown>): void {
  const queries = params.queries;
  const items =
    queries && Array.isArray(queries) && queries.length > 0
      ? (queries as Array<Record<string, unknown>>)
      : [params];

  for (const item of items) {
    const repos = extractRepoOwnerFromParams(item);
    if (repos.length === 0 && !_deps.isLocalTool?.(toolName)) {
      continue;
    }
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
