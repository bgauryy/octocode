/**
 * Register the githubCloneRepo tool with the MCP server.
 *
 * This tool enables AI agents to clone (or partially fetch) a GitHub
 * repository so that local filesystem tools and LSP semantic tools can
 * analyse the code offline. Clones are cached for 24 hours.
 */

import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod/v4';
import type { CloneRepoQuerySchema } from '@octocodeai/octocode-core/schemas';

type CloneRepoQuery = z.infer<typeof CloneRepoQuerySchema>;
import {
  BulkCloneRepoLocalSchema,
  GitHubCloneRepoOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { executeCloneRepo } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerGitHubCloneRepoTool =
  createRemoteToolRegistration<CloneRepoQuery>({
    name: TOOL_NAMES.GITHUB_CLONE_REPO,
    title: 'Clone / Fetch GitHub Repository Locally',
    inputSchema: BulkCloneRepoLocalSchema,
    outputSchema: GitHubCloneRepoOutputLocalSchema,
    executionFn: executeCloneRepo,
    annotations: {
      readOnlyHint: false,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
