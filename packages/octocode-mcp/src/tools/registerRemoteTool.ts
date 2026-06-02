import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { toMCPSchema } from '../types/toolTypes.js';
import { withSecurityValidation } from '../utils/securityBridge.js';
import type { ToolInvocationCallback } from '../types/toolResults.js';
import { DESCRIPTIONS } from './toolMetadata/proxies.js';
import { invokeCallbackSafely } from './utils.js';
import type { ToolExecutionArgs } from '../types/execution.js';
import { logSessionError } from '../session.js';

interface RemoteToolConfig<TQuery> {
  /** Tool name (must be a key in TOOL_NAMES) */
  name: string;
  /** Human-readable title for MCP annotations */
  title: string;
  /** Zod input schema for validation */
  inputSchema: object;
  /** Zod output schema for structured content */
  outputSchema: object;
  /** The execution function that processes bulk queries */
  executionFn: (args: ToolExecutionArgs<TQuery>) => Promise<CallToolResult>;
  /** Optional transform applied to the upstream tool description */
  describe?: (base: string) => string;
  /** MCP tool annotations (defaults provided for typical read-only tools) */
  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };
  /**
   * Optional async pre-flight check. When provided and resolving to false,
   * the tool is NOT registered and the registration function returns null.
   * Used by package_search (npm availability + registry reachable) and any
   * other tool that needs an environment probe before announcing itself.
   */
  registrationGuard?: () => Promise<boolean>;
}

/**
 * Create a registration function for a remote tool.
 *
 * Handles the common pattern shared by all remote tools:
 * 1. Register with MCP server using name, description, schemas
 * 2. Wrap handler with withSecurityValidation
 * 3. Invoke callback safely
 * 4. Assemble ToolExecutionArgs and delegate to executionFn
 */
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
          outputSchema: toMCPSchema(outputSchema),
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
        // Surface the skip. A guard returning false (e.g. npm or the npm
        // registry being unreachable for packageSearch) otherwise drops the
        // tool from the server silently, making "why is this tool missing?"
        // undiagnosable. (#T4)
        void logSessionError(
          name,
          'registration-skipped: registrationGuard returned false (precondition unmet)'
        );
        return null;
      });
    }
    return doRegister();
  };
}
