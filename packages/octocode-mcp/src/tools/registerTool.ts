import {
  McpServer,
  type CallToolResult,
  type RegisteredTool,
  type StandardSchemaWithJSON,
} from '@modelcontextprotocol/server';
import {
  invokeCallbackSafely,
  withBasicSecurityValidation,
  withSecurityValidation,
  type BulkResponsePagination,
  type ToolDirectSecurity,
  type ToolExecutionArgs,
  type ToolInvocationCallback,
} from '@octocodeai/octocode-tools-core';

interface ToolRegistrationInput<TQuery> extends BulkResponsePagination {
  queries?: TQuery[];
}

interface ToolRegistrationConfig<TQuery> {
  name: string;
  title: string;
  description: string;
  inputSchema: object;
  outputSchema: object;
  executionFn: (args: ToolExecutionArgs<TQuery>) => Promise<CallToolResult>;
  security: ToolDirectSecurity;
  timeoutMs?: number;
  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };
}

export function createToolRegistration<TQuery>(
  config: ToolRegistrationConfig<TQuery>
): (server: McpServer, callback?: ToolInvocationCallback) => RegisteredTool {
  return (server: McpServer, callback?: ToolInvocationCallback) => {
    const annotations = {
      title: config.title,
      readOnlyHint: config.annotations?.readOnlyHint ?? true,
      destructiveHint: config.annotations?.destructiveHint ?? false,
      idempotentHint: config.annotations?.idempotentHint ?? true,
      openWorldHint:
        config.annotations?.openWorldHint ?? config.security === 'remote',
    };
    const descriptor = {
      title: config.title,
      description: config.description,
      inputSchema: config.inputSchema as StandardSchemaWithJSON,
      outputSchema: config.outputSchema as StandardSchemaWithJSON,
      annotations,
    };

    if (config.security === 'basic') {
      return server.registerTool(
        config.name,
        descriptor,
        withBasicSecurityValidation(
          async (args: ToolRegistrationInput<TQuery>, context) => {
            const queries = args.queries ?? [];
            await invokeCallbackSafely(callback, config.name, queries);
            return config.executionFn({
              queries,
              responseCharOffset: args.responseCharOffset,
              responseCharLength: args.responseCharLength,
              responseSnapshot: args.responseSnapshot,
              signal: context.signal,
            });
          },
          config.name,
          { timeoutMs: config.timeoutMs }
        )
      );
    }

    return server.registerTool(
      config.name,
      descriptor,
      withSecurityValidation(
        config.name,
        async (
          args: ToolRegistrationInput<TQuery>,
          context
        ): Promise<CallToolResult> => {
          const queries = args.queries || [];
          await invokeCallbackSafely(callback, config.name, queries);
          return config.executionFn({
            queries,
            responseCharOffset: args.responseCharOffset,
            responseCharLength: args.responseCharLength,
            responseSnapshot: args.responseSnapshot,
            authInfo: context.authInfo,
            sessionId: context.sessionId,
            signal: context.signal,
          });
        },
        { timeoutMs: config.timeoutMs }
      )
    );
  };
}
