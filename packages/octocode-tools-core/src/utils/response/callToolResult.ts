import type { CallToolResult } from '@modelcontextprotocol/server';
import { ContentSanitizer } from '@octocodeai/octocode-engine/security';
import { sanitizeStructuredContent } from '../../responses.js';
import { normalizeError } from './normalizedError.js';

// Fail-CLOSED egress policy: sanitization here is the LAST (and for some
// content, the only — e.g. ripgrep snippets) barrier before content leaves the
// process. A sanitizer crash must withhold the affected content, never pass it
// through raw — the moment the scanner is broken is exactly when leaking is
// most likely. Mirrors secureServer's loud-failure policy at the item level.
const WITHHELD_TEXT =
  '[content withheld: sanitization failed — retry the call; if this persists, report it]';

/** Internal transport projection; never a tool-input or global runtime setting. */
export type CallToolResultProjection = 'full' | 'structured';

export function sanitizeCallToolResult(
  result: CallToolResult,
  projection: CallToolResultProjection = 'full'
): CallToolResult {
  // Compact CLI emits only structuredContent when present. Remove the unused
  // text channel BEFORE egress scanning, but keep the same fail-closed scan of
  // every structured leaf. Text-only results need their full fallback; errors
  // retain text because the CLI also uses it to classify the exit code.
  let sanitized =
    projection === 'structured' &&
    result.structuredContent != null &&
    !result.isError
      ? { ...result, content: [] }
      : result;

  if (sanitized.structuredContent) {
    try {
      sanitized = {
        ...sanitized,
        structuredContent: sanitizeStructuredContent(
          sanitized.structuredContent
        ) as Record<string, unknown>,
      };
    } catch {
      sanitized = {
        ...sanitized,
        structuredContent: {
          results: [],
          status: 'error',
          code: 'SANITIZATION_FAILED',
          error:
            'structuredContent withheld: sanitization failed — retry the call; if this persists, report it',
        },
        isError: true,
      };
    }
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
            return { ...item, text: WITHHELD_TEXT };
          }
        }
        return item;
      }),
    };
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
      results: [],
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
