import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type { ToolResult } from 'octocode-security';
import {
  withSecurityValidation as _wsv,
  withBasicSecurityValidation as _wbsv,
} from 'octocode-security';

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
  const inner = _wsv<T, AuthInfo>(
    toolName,
    (sanitizedArgs, authInfo, sessionId) =>
      toolHandler(sanitizedArgs, authInfo, sessionId) as Promise<ToolResult>
  );
  return (args, extra) => inner(args, extra) as Promise<CallToolResult>;
}

export function withBasicSecurityValidation<T extends object>(
  toolHandler: (sanitizedArgs: T) => Promise<CallToolResult>,
  toolName?: string
): (
  args: unknown,
  extra?: { signal?: AbortSignal }
) => Promise<CallToolResult> {
  const inner = _wbsv<T>(
    sanitizedArgs => toolHandler(sanitizedArgs) as Promise<ToolResult>,
    toolName
  );
  return (args, extra) => inner(args, extra) as Promise<CallToolResult>;
}
