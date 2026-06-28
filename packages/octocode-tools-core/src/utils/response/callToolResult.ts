import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/security';
import { sanitizeStructuredContent } from '../../responses.js';
import { getRuntimeSurface } from '../../shared/config/runtimeSurface.js';

const FULL_MCP_TEXT_ENV = 'OCTOCODE_MCP_FULL_TEXT';

export function sanitizeCallToolResult(result: CallToolResult): CallToolResult {
  let sanitized = result;

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

  if (shouldCompactMcpText(sanitized)) {
    return compactMcpTextContent(sanitized);
  }

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
            const { content: text } = ContentSanitizer.sanitizeContent(
              item.text
            );
            return { ...item, text };
          } catch {
            return item;
          }
        }
        return item;
      }),
    };
  }

  return sanitized;
}

function shouldCompactMcpText(result: CallToolResult): boolean {
  return (
    getRuntimeSurface() === 'mcp' &&
    process.env[FULL_MCP_TEXT_ENV] !== 'true' &&
    result.isError !== true &&
    result.structuredContent !== undefined
  );
}

function compactMcpTextContent(result: CallToolResult): CallToolResult {
  const nonTextContent = (result.content ?? []).filter(
    item => item.type !== 'text'
  );
  return {
    ...result,
    content: [
      {
        type: 'text',
        text: summarizeStructuredContent(result.structuredContent),
      },
      ...nonTextContent,
    ],
  };
}

function summarizeStructuredContent(value: unknown): string {
  const parts = ['structuredContent available'];
  if (isRecord(value)) {
    if (typeof value.status === 'string') {
      parts.push(`status=${value.status}`);
    }

    if (Array.isArray(value.results)) {
      parts.push(`results=${value.results.length}`);
      const statusCounts = countResultStatuses(value.results);
      if (statusCounts.error > 0) parts.push(`errors=${statusCounts.error}`);
      if (statusCounts.empty > 0) parts.push(`empty=${statusCounts.empty}`);
    }

    const pagination = value.pagination;
    if (isRecord(pagination) && typeof pagination.hasMore === 'boolean') {
      parts.push(`hasMore=${pagination.hasMore}`);
    }
  }

  return `${parts.join(' · ')}. Read structuredContent for full data.`;
}

function countResultStatuses(results: unknown[]): {
  error: number;
  empty: number;
} {
  let error = 0;
  let empty = 0;
  for (const result of results) {
    if (!isRecord(result)) continue;
    if (result.status === 'error') error += 1;
    if (result.status === 'empty') empty += 1;
  }
  return { error, empty };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
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
