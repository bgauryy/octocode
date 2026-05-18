/**
 * Register the githubCloneRepo tool with the MCP server.
 *
 * This tool enables AI agents to clone (or partially fetch) a GitHub
 * repository so that local filesystem tools and LSP semantic tools can
 * analyse the code offline. Clones are cached for 24 hours.
 */

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolInvocationCallback } from '../../types.js';
import {
  GITHUB_CLONE_REPO_DESCRIPTION,
  type CloneRepoQuery,
} from '@octocodeai/octocode-core';
import { BulkCloneRepoLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { executeCloneRepo } from './execution.js';
import { withSecurityValidation } from '../../utils/securityBridge.js';
import { GitHubCloneRepoOutputSchema } from '@octocodeai/octocode-core';
import { invokeCallbackSafely } from '../utils.js';

export function registerGitHubCloneRepoTool(
  server: McpServer,
  callback?: ToolInvocationCallback
) {
  return server.registerTool(
    TOOL_NAMES.GITHUB_CLONE_REPO,
    {
      description: GITHUB_CLONE_REPO_DESCRIPTION,
      inputSchema: toMCPSchema(BulkCloneRepoLocalSchema),
      outputSchema: toMCPSchema(GitHubCloneRepoOutputSchema),
      annotations: {
        title: 'Clone / Fetch GitHub Repository Locally',
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    withSecurityValidation(
      TOOL_NAMES.GITHUB_CLONE_REPO,
      async (args, authInfo, sessionId) => {
        const { queries, responseCharOffset, responseCharLength } = args as {
          queries: CloneRepoQuery[];
          responseCharOffset?: number;
          responseCharLength?: number;
        };

        await invokeCallbackSafely(
          callback,
          TOOL_NAMES.GITHUB_CLONE_REPO,
          queries
        );

        return executeCloneRepo({
          queries,
          responseCharOffset,
          responseCharLength,
          authInfo,
          sessionId,
        });
      }
    )
  );
}
