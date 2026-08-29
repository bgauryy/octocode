// MCP registration for ghSearchDiscussions (opt-in, GraphQL-only). Reuses the
// bulk schema + executor from octocode-tools-core; gating is inherited from the
// core tool catalog, so this only supplies the registration handle.
import type { z } from 'zod';
import type { SearchDiscussionsQuerySchema } from '@octocodeai/octocode-tools-core';
import {
  TOOL_NAMES,
  SearchDiscussionsBulkLocalSchema,
  searchMultipleGitHubDiscussions,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubDiscussionsTool = createRemoteToolRegistration<
  z.input<typeof SearchDiscussionsQuerySchema>
>({
  name: TOOL_NAMES.GITHUB_DISCUSSIONS,
  title: 'GitHub Discussions Search',
  inputSchema: SearchDiscussionsBulkLocalSchema,
  executionFn: searchMultipleGitHubDiscussions,
});
