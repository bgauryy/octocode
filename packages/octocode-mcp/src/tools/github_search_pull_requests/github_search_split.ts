// MCP registrations for the 4 focused tools that replace ghHistoryResearch.
// Each reuses the shared bulk schema + thin `type`-injecting executor from
// octocode-tools-core; the backend/router is unchanged.
import type { z } from 'zod';
import type {
  SearchPullRequestsQuerySchema,
  SearchIssuesQuerySchema,
  SearchCommitsQuerySchema,
  ListReleasesQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import {
  TOOL_NAMES,
  SearchPullRequestsBulkLocalSchema,
  SearchIssuesBulkLocalSchema,
  SearchCommitsBulkLocalSchema,
  ListReleasesBulkLocalSchema,
  searchMultipleGitHubPullRequestsSplit,
  searchMultipleGitHubIssues,
  searchMultipleGitHubCommits,
  listMultipleGitHubReleases,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubPullRequestsSplitTool =
  createRemoteToolRegistration<z.input<typeof SearchPullRequestsQuerySchema>>({
    name: TOOL_NAMES.GITHUB_PULL_REQUESTS,
    title: 'GitHub Pull Request Search',
    inputSchema: SearchPullRequestsBulkLocalSchema,
    executionFn: searchMultipleGitHubPullRequestsSplit,
  });

export const registerSearchGitHubIssuesTool =
  createRemoteToolRegistration<z.input<typeof SearchIssuesQuerySchema>>({
    name: TOOL_NAMES.GITHUB_ISSUES,
    title: 'GitHub Issue Search',
    inputSchema: SearchIssuesBulkLocalSchema,
    executionFn: searchMultipleGitHubIssues,
  });

export const registerSearchGitHubCommitsTool =
  createRemoteToolRegistration<z.input<typeof SearchCommitsQuerySchema>>({
    name: TOOL_NAMES.GITHUB_COMMITS,
    title: 'GitHub Commit History',
    inputSchema: SearchCommitsBulkLocalSchema,
    executionFn: searchMultipleGitHubCommits,
  });

export const registerListGitHubReleasesTool =
  createRemoteToolRegistration<z.input<typeof ListReleasesQuerySchema>>({
    name: TOOL_NAMES.GITHUB_RELEASES,
    title: 'GitHub Releases',
    inputSchema: ListReleasesBulkLocalSchema,
    executionFn: listMultipleGitHubReleases,
  });
