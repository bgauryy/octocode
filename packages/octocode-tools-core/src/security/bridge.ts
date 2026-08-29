import type {
  AuthInfo,
  CallToolResult,
  ServerContext,
} from '@modelcontextprotocol/server';
import type {
  ToolResult,
  ToolSecurityContext,
} from '@octocodeai/octocode-engine/security';
import {
  withSecurityValidation as _wsv,
  withBasicSecurityValidation as _wbsv,
} from '@octocodeai/octocode-engine/security';

export function withSecurityValidation<T extends Record<string, unknown>>(
  toolName: string,
  toolHandler: (
    sanitizedArgs: T,
    context: ToolSecurityContext<AuthInfo>
  ) => Promise<CallToolResult>,
  options?: { timeoutMs?: number }
): (args: unknown, context?: ServerContext) => Promise<CallToolResult> {
  const inner = _wsv<T, AuthInfo>(
    toolName,
    (sanitizedArgs, context) =>
      toolHandler(sanitizedArgs, context) as Promise<ToolResult>,
    options
  );
  return (args, context) =>
    inner(args, {
      authInfo: context?.http?.authInfo,
      sessionId: context?.sessionId,
      signal: context?.mcpReq.signal,
    }) as Promise<CallToolResult>;
}

export function withBasicSecurityValidation<T extends object>(
  toolHandler: (
    sanitizedArgs: T,
    context: ToolSecurityContext<unknown>
  ) => Promise<CallToolResult>,
  toolName?: string,
  options?: { timeoutMs?: number }
): (args: unknown, context?: ServerContext) => Promise<CallToolResult> {
  const inner = _wbsv<T>(
    (sanitizedArgs, context) =>
      toolHandler(sanitizedArgs, context) as Promise<ToolResult>,
    toolName,
    options
  );
  return (args, context) =>
    inner(args, { signal: context?.mcpReq.signal }) as Promise<CallToolResult>;
}
