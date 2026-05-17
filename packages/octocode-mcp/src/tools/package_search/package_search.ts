import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withSecurityValidation } from '../../utils/securityBridge.js';
import type { ToolInvocationCallback } from '../../types.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import type { PackageSearchQuery } from '@octocodeai/octocode-core';
import { PackageSearchBulkQueryLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { invokeCallbackSafely } from '../utils.js';
import { checkNpmAvailability } from '../../utils/exec/npm.js';
import { checkNpmRegistryReachable } from '../../utils/package/npm.js';
import { searchPackages } from './execution.js';
import { PackageSearchOutputSchema } from '@octocodeai/octocode-core';

export async function registerPackageSearchTool(
  server: McpServer,
  callback?: ToolInvocationCallback
): Promise<RegisteredTool | null> {
  const npmAvailable = await checkNpmAvailability(10000);
  if (!npmAvailable) {
    return null;
  }

  const registryReachable = await checkNpmRegistryReachable();
  if (!registryReachable) {
    return null;
  }

  return server.registerTool(
    TOOL_NAMES.PACKAGE_SEARCH,
    {
      description: DESCRIPTIONS[TOOL_NAMES.PACKAGE_SEARCH],
      inputSchema: toMCPSchema(PackageSearchBulkQueryLocalSchema),
      outputSchema: toMCPSchema(PackageSearchOutputSchema),
      annotations: {
        title: 'Package Search',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    withSecurityValidation(
      TOOL_NAMES.PACKAGE_SEARCH,
      async (
        args: {
          queries: PackageSearchQuery[];
          responseCharOffset?: number;
          responseCharLength?: number;
        },
        _authInfo,
        _sessionId
      ): Promise<CallToolResult> => {
        const queries = args.queries || [];

        await invokeCallbackSafely(
          callback,
          TOOL_NAMES.PACKAGE_SEARCH,
          queries
        );

        return searchPackages({
          queries,
          responseCharOffset: args.responseCharOffset,
          responseCharLength: args.responseCharLength,
        });
      }
    )
  );
}
