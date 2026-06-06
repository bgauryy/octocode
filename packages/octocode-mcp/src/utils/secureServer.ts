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
      void 0;
    }
  }

  return sanitized;
}

const TOOL_CALLBACK_EXCEPTION = 'TOOL_CALLBACK_EXCEPTION';

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
