import type { AuthInfo } from '@modelcontextprotocol/server';
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
import { countPaginationMetadata } from './paginationMetadata.js';

export function transformPullRequestResult(
  data: GitHubPullRequestSearchApiData,
  query: PullRequestQuery,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): PullRequestSearchResult {
  const items: PullRequestItem[] = (data.pullRequests || []).map(
    (pr: GitHubPullRequestApiItem) => ({
      number: pr.number,
      title: pr.title,
      body: pr.body || null,
      ...(pr.bodyPagination && { bodyPagination: pr.bodyPagination }),
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
      sourceBranch: pr.headRef || '',
      targetBranch: pr.baseRef || '',
      sourceSha: pr.headSha,
      targetSha: pr.baseSha,
      createdAt: pr.createdAt,
      updatedAt: pr.updatedAt,
      closedAt: pr.closedAt,
      mergedAt: pr.mergedAt,
      commentsCount: pr.comments,
      changedFilesCount: pr.changedFiles,
      additions: pr.additions,
      deletions: pr.deletions,
      comments: pr.commentDetails?.map(c => ({
        id: c.id,
        author: c.user,
        body: c.body,
        ...(c.bodyPagination && { bodyPagination: c.bodyPagination }),
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
        ...(c.commentType && { commentType: c.commentType }),
        ...(c.path && { path: c.path }),
        ...(c.line !== undefined && { line: c.line }),
        ...(c.inReplyToId != null && { inReplyToId: c.inReplyToId }),
      })) as PullRequestItem['comments'],
      reviews: pr.reviews?.map(review => ({
        id: review.id,
        user: review.user,
        state: review.state,
        body: review.body,
        submittedAt: review.submittedAt,
        commitId: review.commitId,
      })),
      commits: pr.commitDetails?.map(c => ({
        sha: c.sha,
        message: c.message,
        author: c.author,
        date: c.date,
      })),
      fileChanges: pr.fileChanges?.map(f => ({
        path: f.filename,
        status: f.status,
        additions: f.additions,
        deletions: f.deletions,
        patch: f.patch,
      })),
      ...(Array.isArray(pr.sanitizationWarnings) &&
      pr.sanitizationWarnings.length > 0
        ? { sanitizationWarnings: pr.sanitizationWarnings as string[] }
        : {}),
    })
  );

  const { owner: projectOwner, repo } = query.projectId
    ? parseProjectId(query.projectId)
    : { owner: undefined, repo: undefined };
  const owner = projectOwner || query.owner;

  return {
    items,
    totalCount: data.totalCount || items.length,
    pagination: {
      currentPage: data.pagination?.currentPage || 1,
      totalPages: data.pagination?.totalPages || 1,
      hasMore: data.pagination?.hasMore || false,
      ...(data.pagination?.hasMore
        ? { nextPage: (data.pagination?.currentPage || 1) + 1 }
        : {}),
      totalMatches: data.pagination?.totalMatches,
      entriesPerPage: data.pagination?.perPage,
      ...countPaginationMetadata(data.pagination),
    },
    ...((data as { effectiveQuery?: string }).effectiveQuery
      ? { effectiveQuery: (data as { effectiveQuery?: string }).effectiveQuery }
      : {}),
    repositoryContext: owner && repo ? { owner, repo } : undefined,
  };
}

/**
 * Provider-shape query → GitHub search params. Exported as its own seam:
 * every field the search-vs-listing dispatcher (`shouldUseSearchForPRs`) or
 * the query builder reads MUST be mapped here — `query` (keywords) was once
 * omitted, so keyword searches silently fell back to a plain `pulls.list`
 * listing presented as matches.
 */
export function buildGitHubPullRequestsSearchParams(
  query: PullRequestQuery,
  owner: string | undefined,
  repo: string | undefined
): GitHubPullRequestsSearchParams {
  return {
    owner,
    repo,
    query: query.query,
    prNumber: query.number,
    state:
      query.state === 'merged'
        ? 'closed'
        : query.state === 'all'
          ? undefined
          : query.state,
    merged: query.state === 'merged' ? true : undefined,
    draft: query.draft,
    author: query.author,
    assignee: query.assignee,
    commenter: query.commenter,
    mentions: query.mentions,
    'reviewed-by': query.reviewedBy,
    'review-requested': query.reviewRequested,
    label: query.labels,
    base: query.baseBranch,
    head: query.headBranch,
    created: query.created,
    updated: query.updated,
    closed: query.closed,
    'merged-at': query.mergedAt,
    comments: query.comments,
    reactions: query.reactions,
    match: query.match,
    checks: query.checks,
    review: query.review,
    archived: query.archived,
    content: query.content,
    filePage: query.filePage,
    commentPage: query.commentPage,
    commitPage: query.commitPage,
    itemsPerPage: query.itemsPerPage,
    sort: query.sort,
    order: query.order,
    limit: query.limit,
    page: query.page,
    charOffset: query.charOffset,
    charLength: query.charLength,
  };
}

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

  const githubParams = buildGitHubPullRequestsSearchParams(query, owner, repo);

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
