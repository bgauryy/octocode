import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
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

  outputSchema?: object;

  executionFn: (args: ToolExecutionArgs<TQuery>) => Promise<CallToolResult>;

  describe?: (base: string) => string;

  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };

  registrationGuard?: () => Promise<boolean>;
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
    outputSchema,
    executionFn,
    describe,
    annotations,
    registrationGuard,
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
          ...(outputSchema ? { outputSchema: toMCPSchema(outputSchema) } : {}),
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
            authInfo,
            sessionId
          ): Promise<CallToolResult> => {
            const queries = args.queries || [];

            await invokeCallbackSafely(callback, name, queries);

            return executionFn({
              queries,
              responseCharOffset: args.responseCharOffset,
              responseCharLength: args.responseCharLength,
              authInfo,
              sessionId,
            });
          }
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
