/**
 * Unified output sanitization layer for all MCP tool callbacks.
 *
 * Wraps `McpServer.registerTool` via a transparent Proxy so every tool's
 * `CallToolResult` is sanitized before reaching the MCP transport — both
 * `content[]` text blocks and `structuredContent` deep-walked strings.
 *
 * Call `withOutputSanitization(server)` once, then pass the returned proxy
 * to all tool registration functions.
 */

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { ContentSanitizer } from 'octocode-security-utils';
import { maskSensitiveData } from 'octocode-security-utils/mask';
import { sanitizeStructuredContent } from '../responses.js';

/**
 * Sanitize a `CallToolResult`:
 * 1. Redact secrets in `content[]` text items via `ContentSanitizer` + `maskSensitiveData`
 * 2. Deep-walk `structuredContent` strings via `sanitizeStructuredContent`
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
          const scan = ContentSanitizer.sanitizeContent(item.text);
          if (scan.hasSecrets) {
            return { ...item, text: maskSensitiveData(scan.content) };
          }
          return { ...item, text: maskSensitiveData(item.text) };
        }
        return item;
      }),
    };
  }

  if (sanitized.structuredContent) {
    sanitized = {
      ...sanitized,
      structuredContent: sanitizeStructuredContent(
        sanitized.structuredContent
      ) as Record<string, unknown>,
    };
  }

  return sanitized;
}

/**
 * Return a Proxy around `server` that intercepts `registerTool` calls,
 * wrapping every callback's return value through `sanitizeCallToolResult`.
 *
 * The original `server.registerTool` is never replaced, so test spies
 * and the SDK's internal state are preserved.
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
          const wrappedCb = async (...args: unknown[]) => {
            const result = await (
              cb as (...a: unknown[]) => Promise<CallToolResult>
            )(...args);
            return sanitizeCallToolResult(result);
          };
          return target.registerTool(name, config as never, wrappedCb as never);
        };
      }
      return Reflect.get(target, prop, receiver);
    },
  });
}
