import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod';
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
