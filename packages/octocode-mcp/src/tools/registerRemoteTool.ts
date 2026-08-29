import { McpServer, type RegisteredTool } from '@modelcontextprotocol/server';
import type { CallToolResult } from '@modelcontextprotocol/server';
import { toMCPSchema } from '../types/toolTypes.js';
import { withSecurityValidation } from '@octocodeai/octocode-tools-core';
import {
  DESCRIPTIONS,
  invokeCallbackSafely,
} from '@octocodeai/octocode-tools-core';
import type {
  ToolInvocationCallback,
  ToolExecutionArgs,
} from '@octocodeai/octocode-tools-core';

interface RemoteToolConfig<TQuery> {
  name: string;

  title: string;

  inputSchema: object;

  executionFn: (args: ToolExecutionArgs<TQuery>) => Promise<CallToolResult>;

  describe?: (base: string) => string;

  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };

  registrationGuard?: () => Promise<boolean>;

  timeoutMs?: number;
}

export function createRemoteToolRegistration<TQuery>(
  config: RemoteToolConfig<TQuery>
): (
  server: McpServer,
  callback?: ToolInvocationCallback
) => RegisteredTool | Promise<RegisteredTool | null> {
  const {
    name,
    title,
    inputSchema,
    executionFn,
    describe,
    annotations,
    registrationGuard,
    timeoutMs,
  } = config;

  return (server: McpServer, callback?: ToolInvocationCallback) => {
    const doRegister = (): RegisteredTool => {
      const baseDescription = DESCRIPTIONS[name] ?? '';
      const description = describe
        ? describe(baseDescription)
        : baseDescription;
      return server.registerTool(
        name,
        {
          description,
          inputSchema: toMCPSchema(inputSchema),
          annotations: {
            title,
            readOnlyHint: annotations?.readOnlyHint ?? true,
            destructiveHint: annotations?.destructiveHint ?? false,
            idempotentHint: annotations?.idempotentHint ?? true,
            openWorldHint: annotations?.openWorldHint ?? true,
          },
        },
        withSecurityValidation(
          name,
          async (
            args: {
              queries: TQuery[];
              responseCharOffset?: number;
              responseCharLength?: number;
            },
            context
          ): Promise<CallToolResult> => {
            const queries = args.queries || [];

            await invokeCallbackSafely(callback, name, queries);

            return executionFn({
              queries,
              responseCharOffset: args.responseCharOffset,
              responseCharLength: args.responseCharLength,
              authInfo: context.authInfo,
              sessionId: context.sessionId,
              signal: context.signal,
            });
          },
          { timeoutMs }
        )
      );
    };

    if (registrationGuard) {
      return registrationGuard().then(ok => {
        if (ok) return doRegister();
        return null;
      });
    }
    return doRegister();
  };
}
