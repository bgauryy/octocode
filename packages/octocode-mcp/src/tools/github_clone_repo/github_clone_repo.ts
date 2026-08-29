import type { z } from 'zod';
import type { CloneRepoQuerySchema } from '@octocodeai/octocode-tools-core';
import {
  TOOL_NAMES,
  BulkCloneRepoLocalSchema,
  executeCloneRepo,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

type CloneRepoQuery = z.infer<typeof CloneRepoQuerySchema>;

export const registerGitHubCloneRepoTool =
  createRemoteToolRegistration<CloneRepoQuery>({
    name: TOOL_NAMES.GITHUB_CLONE_REPO,
    title: 'Clone / Fetch GitHub Repository Locally',
    inputSchema: BulkCloneRepoLocalSchema,
    executionFn: executeCloneRepo,
    timeoutMs: 150_000,
    annotations: {
      readOnlyHint: false,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
