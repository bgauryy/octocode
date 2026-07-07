import type { z } from 'zod';
import type { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubPullRequestSearchQuery = z.input<
  typeof GitHubPullRequestSearchQuerySchema
>;
import {
  TOOL_NAMES,
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubSearchPullRequestsOutputLocalSchema,
  searchMultipleGitHubPullRequests,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubPullRequestsTool =
  createRemoteToolRegistration<GitHubPullRequestSearchQuery>({
    name: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    title: 'GitHub Pull Request Search',
    inputSchema: GitHubPullRequestSearchBulkQueryLocalSchema,
    outputSchema: GitHubSearchPullRequestsOutputLocalSchema,
    executionFn: searchMultipleGitHubPullRequests,
  });
