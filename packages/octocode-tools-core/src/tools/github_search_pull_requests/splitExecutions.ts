import type { CallToolResult } from '@modelcontextprotocol/server';

// Thin internal executors for focused GitHub history and release modes. Each injects
// the mode `type` its tool owns, then delegates to the shared, already-tested
// bulk executor + router (searchMultipleGitHubPullRequests). This keeps ONE
// backend while giving the agent four focused, single-purpose tools.
import type { ToolExecutionArgs } from '../../types/execution.js';
import type { GitHubPullRequestSearchInput } from './execution/types.js';
import { searchMultipleGitHubPullRequests } from './execution.js';

type SplitArgs = ToolExecutionArgs<Record<string, unknown>>;

function withType(
  args: SplitArgs,
  type: 'prs' | 'issues' | 'commits' | 'releases'
): ToolExecutionArgs<GitHubPullRequestSearchInput> {
  return {
    ...args,
    queries: (args.queries as Array<Record<string, unknown>>).map(q => ({
      ...q,
      type,
    })),
  } as ToolExecutionArgs<GitHubPullRequestSearchInput>;
}

export function searchMultipleGitHubPullRequestsSplit(
  args: SplitArgs
): Promise<CallToolResult> {
  return searchMultipleGitHubPullRequests(withType(args, 'prs'));
}

export function searchMultipleGitHubIssues(
  args: SplitArgs
): Promise<CallToolResult> {
  return searchMultipleGitHubPullRequests(withType(args, 'issues'));
}

export function searchMultipleGitHubCommits(
  args: SplitArgs
): Promise<CallToolResult> {
  return searchMultipleGitHubPullRequests(withType(args, 'commits'));
}

export function listMultipleGitHubReleases(
  args: SplitArgs
): Promise<CallToolResult> {
  return searchMultipleGitHubPullRequests(withType(args, 'releases'));
}
