/**
 * MCP-typed bridge for octocode-security-utils withSecurityValidation.
 *
 * octocode-security-utils returns the framework-agnostic `ToolResult` type.
 * The MCP SDK's server.tool() expects `CallToolResult`.  These two types
 * are structurally equivalent at runtime but TypeScript cannot infer the
 * relationship because MCP uses string-literal discriminants.
 *
 * This file wraps the agnostic helpers with correct MCP return types so
 * the rest of octocode-mcp doesn't need any `as unknown as` casts.
 */

import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import {
  withSecurityValidation as _wsv,
  withBasicSecurityValidation as _wbsv,
  configureSecurity,
} from 'octocode-security-utils';

export { configureSecurity };

export function withSecurityValidation<T extends Record<string, unknown>>(
  toolName: string,
  toolHandler: (
    sanitizedArgs: T,
    authInfo?: AuthInfo,
    sessionId?: string
  ) => Promise<CallToolResult>
): (
  args: unknown,
  extra: { authInfo?: AuthInfo; sessionId?: string; signal?: AbortSignal }
) => Promise<CallToolResult> {
  return _wsv<T, AuthInfo>(
    toolName,
    toolHandler as Parameters<typeof _wsv<T, AuthInfo>>[1]
  ) as unknown as (
    args: unknown,
    extra: { authInfo?: AuthInfo; sessionId?: string; signal?: AbortSignal }
  ) => Promise<CallToolResult>;
}

export function withBasicSecurityValidation<T extends object>(
  toolHandler: (sanitizedArgs: T) => Promise<CallToolResult>,
  toolName?: string
): (
  args: unknown,
  extra?: { signal?: AbortSignal }
) => Promise<CallToolResult> {
  return _wbsv<T>(
    toolHandler as Parameters<typeof _wbsv<T>>[0],
    toolName
  ) as unknown as (
    args: unknown,
    extra?: { signal?: AbortSignal }
  ) => Promise<CallToolResult>;
}
