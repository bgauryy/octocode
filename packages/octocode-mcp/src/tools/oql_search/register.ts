import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  executeOqlSearchTool,
  getDescription,
  invokeCallbackSafely,
  OQL_SEARCH_TOOL_NAME,
  OqlSearchInputSchema,
  withSecurityValidation,
} from '@octocodeai/octocode-tools-core';
import type {
  ToolInvocationCallback,
  OqlSearchInput,
} from '@octocodeai/octocode-tools-core';
import { toMCPSchema } from '../../types/toolTypes.js';

export function registerOqlSearchTool(
  server: McpServer,
  callback?: ToolInvocationCallback
): RegisteredTool {
  return server.registerTool(
    OQL_SEARCH_TOOL_NAME,
    {
      description: getDescription(OQL_SEARCH_TOOL_NAME),
      inputSchema: toMCPSchema(OqlSearchInputSchema),
      annotations: {
        title: 'OQL Search',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    withSecurityValidation<Record<string, unknown>>(
      OQL_SEARCH_TOOL_NAME,
      async (args, authInfo, sessionId) => {
        await invokeCallbackSafely(
          callback,
          OQL_SEARCH_TOOL_NAME,
          callbackQueries(args)
        );
        return executeOqlSearchTool({
          ...args,
          authInfo,
          sessionId,
        });
      }
    )
  );
}

function callbackQueries(
  input: OqlSearchInput | Record<string, unknown>
): unknown[] {
  return Array.isArray((input as { queries?: unknown }).queries)
    ? (input as { queries: unknown[] }).queries
    : [input];
}
