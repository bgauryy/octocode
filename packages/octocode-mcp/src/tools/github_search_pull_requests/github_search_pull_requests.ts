import type { z } from 'zod';
import type { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubSearchPullRequestsOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubPullRequests } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubPullRequestsTool =
  createRemoteToolRegistration<GitHubPullRequestSearchQuery>({
    name: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    title: 'GitHub Pull Request Search',
    inputSchema: GitHubPullRequestSearchBulkQueryLocalSchema,
    outputSchema: GitHubSearchPullRequestsOutputLocalSchema,
    executionFn: searchMultipleGitHubPullRequests,
  });
