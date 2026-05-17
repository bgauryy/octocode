/**
 * GitHub Pull Request Search
 *
 * Extracted from GitHubProvider for better modularity.
 *
 * @module providers/github/githubPullRequests
 */

import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  PullRequestQuery,
  PullRequestSearchResult,
  PullRequestItem,
} from '../types.js';

import { searchGitHubPullRequestsAPI } from '../../github/pullRequestSearch.js';

import type { GitHubPullRequestsSearchParams } from '../../github/githubAPI.js';

import type {
  GitHubPullRequestApiItem,
  GitHubPullRequestSearchApiData,
} from '../../tools/github_search_pull_requests/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';
export { parseGitHubProjectId } from './utils.js';

/**
 * Transform GitHub pull request result to unified format.
 */
export function transformPullRequestResult(
  data: GitHubPullRequestSearchApiData,
  query: PullRequestQuery,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): PullRequestSearchResult {
  const items: PullRequestItem[] = (data.pull_requests || []).map(
    (pr: GitHubPullRequestApiItem) => ({
      number: pr.number,
      title: pr.title,
      body: pr.body || null,
      url: pr.url,
      state: pr.merged ? 'merged' : pr.state,
      draft: pr.draft || false,
      author: pr.author,
      assignees:
        pr.assignees?.map(a =>
          typeof a === 'string'
            ? a
            : String((a as Record<string, unknown>).login ?? '')
        ) || [],
      labels:
        pr.labels?.map(l => (typeof l === 'string' ? l : (l.name ?? ''))) || [],
      sourceBranch: pr.head_ref || '',
      targetBranch: pr.base_ref || '',
      sourceSha: pr.head_sha,
      targetSha: pr.base_sha,
      createdAt: pr.created_at,
      updatedAt: pr.updated_at,
      closedAt: pr.closed_at,
      mergedAt: pr.merged_at,
      commentsCount: pr.comments,
      changedFilesCount: pr.changed_files,
      additions: pr.additions,
      deletions: pr.deletions,
      comments: pr.comment_details?.map(c => ({
        id: c.id,
        author: c.user,
        body: c.body,
        createdAt: c.created_at,
        updatedAt: c.updated_at,
      })),
      fileChanges: pr.file_changes?.map(f => ({
        path: f.filename,
        status: f.status,
        additions: f.additions,
        deletions: f.deletions,
        patch: f.patch,
      })),
    })
  );

  const { owner: projectOwner, repo } = query.projectId
    ? parseProjectId(query.projectId)
    : { owner: undefined, repo: undefined };
  const owner = projectOwner || query.owner;

  return {
    items,
    totalCount: data.total_count || items.length,
    pagination: {
      currentPage: data.pagination?.currentPage || 1,
      totalPages: data.pagination?.totalPages || 1,
      hasMore: data.pagination?.hasMore || false,
      totalMatches: data.pagination?.totalMatches,
    },
    repositoryContext: owner && repo ? { owner, repo } : undefined,
  };
}

/**
 * Search pull requests on GitHub.
 */
export async function searchPullRequests(
  query: PullRequestQuery,
  authInfo?: AuthInfo,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): Promise<ProviderResponse<PullRequestSearchResult>> {
  const { owner: projectOwner, repo } = query.projectId
    ? parseProjectId(query.projectId)
    : { owner: undefined, repo: undefined };
  const owner = projectOwner || query.owner;

  const githubParams: GitHubPullRequestsSearchParams = {
    owner,
    repo,
    prNumber: query.number,
    state:
      query.state === 'merged'
        ? 'closed'
        : query.state === 'all'
          ? undefined
          : query.state,
    merged: query.merged ?? (query.state === 'merged' ? true : undefined),
    draft: query.draft,
    author: query.author,
    assignee: query.assignee,
    commenter: query.commenter,
    involves: query.involves,
    mentions: query.mentions,
    'reviewed-by': query.reviewedBy,
    'review-requested': query.reviewRequested,
    label: query.labels,
    'no-assignee': query.noAssignee,
    'no-label': query.noLabel,
    'no-milestone': query.noMilestone,
    'no-project': query.noProject,
    base: query.baseBranch,
    head: query.headBranch,
    created: query.created,
    updated: query.updated,
    closed: query.closed,
    'merged-at': query.mergedAt,
    comments: query.comments,
    reactions: query.reactions,
    interactions: query.interactions,
    match: query.match,
    withComments: query.withComments,
    withCommits: query.withCommits,
    type: query.type,
    partialContentMetadata: query.partialContentMetadata,
    sort: query.sort,
    order: query.order,
    limit: query.limit,
    page: query.page,
  };

  const result = await searchGitHubPullRequestsAPI(githubParams, authInfo);

  if (result.error) {
    return createGitHubProviderError({
      error:
        typeof result.error === 'string' ? result.error : String(result.error),
      status: result.status || 500,
      hints: result.hints,
      rateLimitRemaining: result.rateLimitRemaining,
      rateLimitReset: result.rateLimitReset,
      retryAfter: result.retryAfter,
    });
  }

  return {
    data: transformPullRequestResult(result, query, parseProjectId),
    status: 200,
    provider: 'github',
    rawResponseChars: result.rawResponseChars ?? countSerializedChars(result),
  };
}
