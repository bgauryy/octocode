import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  type CallToolResult,
  McpError,
  ErrorCode,
} from '@modelcontextprotocol/sdk/types.js';
import {
  ContentSanitizer,
  maskSensitiveData,
  sanitizeCallToolResult,
  buildToolErrorResult,
} from '@octocodeai/octocode-tools-core';

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

/**
 * Output sanitization threw. We keep the tool result (availability) but must
 * never let the failure pass silently, since the content was NOT scrubbed for
 * secrets. Prepend a visible warning block so the caller knows the output is
 * unsanitized, and mirror it to stderr. The failure reason is itself scrubbed
 * so the warning can't leak a secret via the error text.
 */
function appendSanitizationWarning(
  result: CallToolResult,
  error: unknown
): CallToolResult {
  const reason = sanitizeErrorMessage(normalizeError(error).message);
  const warning = {
    type: 'text' as const,
    text: `⚠️ Output sanitization failed — the content below was NOT scrubbed for secrets and may contain sensitive data. Reason: ${reason}`,
  };
  const content = Array.isArray(result.content) ? result.content : [];
  try {
    process.stderr.write(
      `[octocode-mcp] output sanitization failed; returning unsanitized result with warning: ${reason}\n`
    );
  } catch {
    // stderr unavailable — the in-band warning above still surfaces it
  }
  return { ...result, content: [warning, ...content] };
}

function safeStringify(value: unknown): string | undefined {
  try {
    return JSON.stringify(value);
  } catch {
    return undefined;
  }
}

function sanitizeErrorMessage(message: string): string {
  try {
    const scan = ContentSanitizer.sanitizeContent(message);
    return maskSensitiveData(scan.hasSecrets ? scan.content : message);
  } catch {
    return message;
  }
}

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
      } catch (sanitizeError) {
        // Sanitization failed. Policy: do NOT hard-fail the call and do NOT
        // return the result silently — the content was not scrubbed for
        // secrets, so surface a visible warning and let the caller decide.
        return appendSanitizationWarning(result, sanitizeError);
      }
    } catch (error) {
      return buildToolErrorResult(name, error);
    }
  };
}

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
      throw new McpError(
        ErrorCode.InternalError,
        `${kind} "${name}" failed: ${safeMessage}`
      );
    }
  };
}

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
          // `as never`: Proxy.get() cannot resolve the SDK's generic registerTool overloads;
          // runtime types are correct — the Proxy intercepts all string props uniformly.
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
