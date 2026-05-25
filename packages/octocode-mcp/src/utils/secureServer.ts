/**
 * Unified output sanitization + crash-isolation layer for all MCP callbacks.
 *
 * Wraps `McpServer.registerTool` / `registerResource`
 * via a transparent Proxy so every callback is:
 * 1. Wrapped in a try/catch — any thrown error (sync, async, or non-Error
 *    rejection) is intercepted so the MCP server can never crash on a
 *    handler exception:
 *    - For tools: converted into a safe `isError: true` `CallToolResult`.
 *      (When `isError` is true, the MCP SDK skips `outputSchema` validation
 *      — see `validateToolOutput` in `@modelcontextprotocol/sdk` — so the
 *      structured error payload below cannot be rejected by a strict tool
 *      output schema.)
 *    - For resources: re-thrown as a sanitized `McpError` so the
 *      JSON-RPC error response carries a redacted message instead of
 *      leaking raw error text (which could contain secrets).
 * 2. Sanitized before reaching the MCP transport — both `content[]` text
 *    blocks and `structuredContent` deep-walked strings.
 *
 * Call `withOutputSanitization(server)` once, then pass the returned proxy
 * to all registration functions (tools, resources).
 */

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  type CallToolResult,
  McpError,
  ErrorCode,
} from '@modelcontextprotocol/sdk/types.js';
import { ContentSanitizer } from 'octocode-security-utils';
import { maskSensitiveData } from 'octocode-security-utils/mask';
import { sanitizeStructuredContent } from '../responses.js';
import { logSessionError } from '../session.js';
import { ignoreBestEffortFailure } from './core/bestEffort.js';

/**
 * Sanitize a `CallToolResult`:
 * 1. Redact secrets in `content[]` text items via `ContentSanitizer` + `maskSensitiveData`
 * 2. Deep-walk `structuredContent` strings via `sanitizeStructuredContent`
 *
 * Sanitization is best-effort: if any step throws, the original (un-sanitized)
 * result is returned rather than propagating the exception. The outer
 * `withOutputSanitization` wrapper provides the final crash safety net.
 */
export function sanitizeCallToolResult(result: CallToolResult): CallToolResult {
  let sanitized = result;

  if (sanitized.content?.length) {
    sanitized = {
      ...sanitized,
      content: sanitized.content.map(item => {
        if (
          item.type === 'text' &&
          'text' in item &&
          typeof item.text === 'string'
        ) {
          try {
            const scan = ContentSanitizer.sanitizeContent(item.text);
            const text = scan.hasSecrets
              ? maskSensitiveData(scan.content)
              : maskSensitiveData(item.text);
            return { ...item, text };
          } catch {
            return item;
          }
        }
        return item;
      }),
    };
  }

  if (sanitized.structuredContent) {
    try {
      sanitized = {
        ...sanitized,
        structuredContent: sanitizeStructuredContent(
          sanitized.structuredContent
        ) as Record<string, unknown>,
      };
    } catch {
      // Keep original structuredContent if sanitization throws.
    }
  }

  return sanitized;
}

/**
 * Generic error code reported when a tool callback throws.
 * Kept here (and not in `errorCodes.ts`) to avoid coupling the crash-safety
 * layer to higher-level error registries.
 */
const TOOL_CALLBACK_EXCEPTION = 'TOOL_CALLBACK_EXCEPTION';

/**
 * Convert any thrown value into a safe, sanitized `CallToolResult`.
 *
 * - Extracts `message` / `name` / `code` from Error-like objects.
 * - Coerces non-Error throws (strings, numbers, plain objects) into a message.
 * - Never re-throws: a fallback minimal payload is returned even if sanitization
 *   itself misbehaves.
 */
export function buildToolErrorResult(
  toolName: string,
  error: unknown
): CallToolResult {
  const normalized = normalizeError(error);

  const fallback: CallToolResult = {
    content: [
      {
        type: 'text',
        text: `error: tool "${toolName}" threw an exception\nmessage: ${normalized.message}`,
      },
    ],
    structuredContent: {
      status: 'error',
      tool: toolName,
      code: TOOL_CALLBACK_EXCEPTION,
      error: {
        name: normalized.name,
        message: normalized.message,
        code: normalized.code,
      },
    },
    isError: true,
  };

  try {
    return sanitizeCallToolResult(fallback);
  } catch {
    return fallback;
  }
}

interface NormalizedError {
  name: string;
  message: string;
  code?: string;
}

function normalizeError(error: unknown): NormalizedError {
  if (error instanceof Error) {
    const code = (error as { code?: unknown }).code;
    return {
      name: error.name || 'Error',
      message: error.message || String(error),
      code: typeof code === 'string' ? code : undefined,
    };
  }

  if (typeof error === 'string') {
    return { name: 'Error', message: error };
  }

  if (error && typeof error === 'object') {
    const obj = error as Record<string, unknown>;
    const message =
      typeof obj.message === 'string'
        ? obj.message
        : (safeStringify(obj) ?? 'Unknown error');
    const name = typeof obj.name === 'string' ? obj.name : 'Error';
    const code = typeof obj.code === 'string' ? obj.code : undefined;
    return { name, message, code };
  }

  return {
    name: 'Error',
    message: error === undefined ? 'undefined' : String(error),
  };
}

function safeStringify(value: unknown): string | undefined {
  try {
    return JSON.stringify(value);
  } catch {
    return undefined;
  }
}

/**
 * Sanitize a free-form error message before it travels over the wire
 * (e.g. via a JSON-RPC `error.message`). Secrets are redacted; remaining
 * text is masked using the same rules as content sanitization.
 *
 * Falls back to the original message if sanitization itself throws so
 * the crash-safety guarantee is preserved end-to-end.
 */
function sanitizeErrorMessage(message: string): string {
  try {
    const scan = ContentSanitizer.sanitizeContent(message);
    return maskSensitiveData(scan.hasSecrets ? scan.content : message);
  } catch {
    return message;
  }
}

/**
 * Build the wrapped callback used by tool registrations.
 *
 * Catches sync + async exceptions and returns a sanitized
 * `isError: true` `CallToolResult` (never re-throws).
 */
function wrapToolCallback(
  name: string,
  cb: (...args: unknown[]) => unknown
): (...args: unknown[]) => Promise<CallToolResult> {
  return async (...args: unknown[]) => {
    try {
      const result = await (
        cb as (...a: unknown[]) => Promise<CallToolResult> | CallToolResult
      )(...args);
      try {
        return sanitizeCallToolResult(result);
      } catch {
        return result;
      }
    } catch (error) {
      void Promise.resolve(
        logSessionError(name, TOOL_CALLBACK_EXCEPTION)
      ).catch(ignoreBestEffortFailure('tool callback exception logging'));
      return buildToolErrorResult(name, error);
    }
  };
}

/**
 * Build the wrapped callback used by resource registrations.
 *
 * `ReadResourceResult` has no `isError` field, so on
 * exception we sanitize the error message and re-throw as `McpError`. The
 * SDK's protocol layer converts this into a JSON-RPC error response, but
 * the message has been redacted so secrets never reach the wire.
 *
 * Successful results are returned as-is (no `content[]`/`structuredContent`
 * shape to sanitize on these channels).
 */
function wrapNonToolCallback<T>(
  kind: 'resource',
  name: string,
  cb: (...args: unknown[]) => unknown
): (...args: unknown[]) => Promise<T> {
  return async (...args: unknown[]) => {
    try {
      return (await Promise.resolve(
        (cb as (...a: unknown[]) => T | Promise<T>)(...args)
      )) as T;
    } catch (error) {
      const normalized = normalizeError(error);
      const safeMessage = sanitizeErrorMessage(normalized.message);
      void Promise.resolve(
        logSessionError(name, `${kind.toUpperCase()}_CALLBACK_EXCEPTION`)
      ).catch(ignoreBestEffortFailure(`${kind} callback exception logging`));
      throw new McpError(
        ErrorCode.InternalError,
        `${kind} "${name}" failed: ${safeMessage}`
      );
    }
  };
}

/**
 * Return a Proxy around `server` that intercepts `registerTool`
 * and `registerResource` calls.
 *
 * For every registered handler the callback is wrapped so that any thrown
 * exception (sync, async, non-Error rejection) is caught:
 * - Tool callbacks → sanitized `isError: true` `CallToolResult`.
 * - Resource callbacks → sanitized `McpError` re-thrown so the SDK
 *   protocol layer emits a JSON-RPC error response without leaking secrets.
 *
 * Successful tool results additionally pass through `sanitizeCallToolResult`
 * to redact secrets in both `content[]` text and `structuredContent`.
 *
 * The original `server.registerTool` / `registerResource`
 * functions are never replaced on the underlying object, so test spies and
 * the SDK's internal state are preserved.
 *
 * Idempotent: wrapping an already-wrapped server is safe — the outer proxy
 * simply delegates to the inner one, which delegates to the real SDK method.
 */
export function withOutputSanitization(server: McpServer): McpServer {
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === 'registerTool') {
        return (
          name: string,
          config: Record<string, unknown>,
          cb: (...args: unknown[]) => unknown
        ) => {
          const wrappedCb = wrapToolCallback(name, cb);
          return target.registerTool(name, config as never, wrappedCb as never);
        };
      }

      if (prop === 'registerResource') {
        return (
          name: string,
          uriOrTemplate: unknown,
          config: Record<string, unknown>,
          cb: (...args: unknown[]) => unknown
        ) => {
          const wrappedCb = wrapNonToolCallback('resource', name, cb);
          return (target.registerResource as (...a: unknown[]) => unknown)(
            name,
            uriOrTemplate,
            config,
            wrappedCb
          );
        };
      }

      return Reflect.get(target, prop, receiver);
    },
  });
}
