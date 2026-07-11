import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { getOctokit } from '../client.js';
import { isBotAuthor } from '../botFilter.js';
import { parseHasMore } from '../history.js';
import type { GitHubAPIResponse } from '../githubAPI.js';
import {
  buildIssueSearchQuery,
  type IssueSearchParams,
} from '../queryBuilders.js';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../../config.js';
import type { FetchIssuesParams, IssueRow, IssuesResult } from './types.js';
import {
  createIssueError,
  firstString,
  hasPullRequestField,
  toIssueRow,
  windowText,
} from './helpers.js';

export async function fetchIssueByNumber(
  params: FetchIssuesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<IssuesResult>> {
  const owner = firstString(params.owner);
  const repo = firstString(params.repo);
  const issueNumber = params.issueNumber;
  if (!owner || !repo || issueNumber == null) {
    return createIssueError(
      'owner, repo, and issueNumber are required for issue detail mode.'
    );
  }

  const octokit = await getOctokit(authInfo);
  const response = await octokit.rest.issues.get({
    owner,
    repo,
    issue_number: issueNumber,
  });

  if (hasPullRequestField(response.data)) {
    return createIssueError(
      `Issue #${issueNumber} is a pull request; use type:"prs" with prNumber:${issueNumber}.`,
      [
        `Retry with { type: "prs", owner: "${owner}", repo: "${repo}", prNumber: ${issueNumber} }.`,
      ]
    );
  }

  const wantBody = params.content?.body !== false;
  const wantComments = params.content?.comments?.discussion === true;
  const includeBots = params.content?.comments?.includeBots === true;

  const row = toIssueRow(response.data);
  const contentPagination: IssueRow['contentPagination'] = {};

  if (wantBody) {
    const rawBody = ContentSanitizer.sanitizeContent(
      response.data.body ?? ''
    ).content;
    const windowed = windowText(rawBody, params.charOffset, params.charLength);
    row.body = windowed.text;
    if (windowed.pagination) contentPagination.body = windowed.pagination;
  }

  if (wantComments) {
    const commentPage = Math.max(1, params.commentPage ?? 1);
    const itemsPerPage = Math.max(1, params.itemsPerPage ?? 30);
    const commentsResult = await octokit.rest.issues.listComments({
      owner,
      repo,
      issue_number: issueNumber,
      per_page: itemsPerPage,
      page: commentPage,
    });
    const kept = includeBots
      ? commentsResult.data
      : commentsResult.data.filter(c => !isBotAuthor(c.user?.login ?? ''));
    row.comments = kept.map(comment => ({
      id: String(comment.id),
      user: comment.user?.login ?? 'unknown',
      body: ContentSanitizer.sanitizeContent(comment.body ?? '').content,
      created_at: comment.created_at ?? '',
      updated_at: comment.updated_at ?? '',
      commentType: 'discussion' as const,
    }));
    const hasMoreComments = parseHasMore(
      commentsResult.headers.link as string | undefined
    );
    contentPagination.comments = {
      currentPage: commentPage,
      itemsPerPage,
      totalComments: row.comments.length,
      hasMore: hasMoreComments,
      ...(hasMoreComments ? { nextCommentPage: commentPage + 1 } : {}),
    };
  }

  if (Object.keys(contentPagination).length > 0) {
    row.contentPagination = contentPagination;
  }

  return {
    data: {
      type: 'issues',
      owner,
      repo,
      issues: [row],
      total_count: 1,
    },
    status: 200,
  };
}

export async function searchIssues(
  searchParams: IssueSearchParams,
  params: FetchIssuesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<IssuesResult>> {
  const owner = firstString(params.owner) ?? '';
  const repo = firstString(params.repo) ?? '';
  const octokit = await getOctokit(authInfo);
  const q = buildIssueSearchQuery(searchParams);
  const perPage = Math.min(
    params.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
    GITHUB_SEARCH_MAX_LIMIT
  );
  const currentPage = params.page ?? 1;
  const sortValue =
    params.sort && params.sort !== 'best-match' ? params.sort : undefined;

  const searchResult = await octokit.rest.search.issuesAndPullRequests({
    q,
    sort: sortValue as
      'comments' | 'reactions' | 'created' | 'updated' | undefined,
    order: params.order || 'desc',
    per_page: perPage,
    page: currentPage,
  });

  const issues = (searchResult.data.items ?? [])
    .filter(item => !hasPullRequestField(item))
    .map(item => toIssueRow(item));

  const totalMatches = searchResult.data.total_count ?? issues.length;
  const hasMore =
    currentPage * perPage < totalMatches && issues.length === perPage;

  return {
    data: {
      type: 'issues',
      owner,
      repo,
      issues: params.concise
        ? issues.map(i => `#${i.number} ${i.title}`)
        : issues,
      total_count: totalMatches,
      effectiveQuery: q,
      ...(searchResult.data.incomplete_results
        ? { incomplete_results: true }
        : {}),
      pagination: {
        currentPage,
        perPage,
        hasMore,
        ...(hasMore ? { nextPage: currentPage + 1 } : {}),
        totalMatches,
        reportedTotalMatches: totalMatches,
        totalMatchesKind: 'reported',
      },
    },
    status: 200,
  };
}

export async function listIssues(
  params: FetchIssuesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<IssuesResult>> {
  const owner = firstString(params.owner);
  const repo = firstString(params.repo);
  if (!owner || !repo) {
    return createIssueError('owner and repo are required for issues mode.');
  }

  const octokit = await getOctokit(authInfo);
  const perPage = Math.min(
    params.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
    GITHUB_SEARCH_MAX_LIMIT
  );
  const currentPage = params.page ?? 1;
  const state =
    params.state === 'open' || params.state === 'closed' ? params.state : 'all';

  const listResult = await octokit.rest.issues.listForRepo({
    owner,
    repo,
    state,
    per_page: perPage,
    page: currentPage,
    ...(params.assignee ? { assignee: params.assignee } : {}),
    ...(params.author ? { creator: params.author } : {}),
    ...(params.mentions ? { mentioned: params.mentions } : {}),
    ...(params.milestone ? { milestone: params.milestone } : {}),
    ...(params.sort === 'created' ||
    params.sort === 'updated' ||
    params.sort === 'comments'
      ? { sort: params.sort }
      : {}),
    ...(params.order ? { direction: params.order } : {}),
    ...(typeof params.label === 'string'
      ? { labels: params.label }
      : Array.isArray(params.label)
        ? { labels: params.label.join(',') }
        : {}),
  });

  const issues = listResult.data
    .filter(item => !hasPullRequestField(item))
    .map(item => toIssueRow(item));

  const hasMore = parseHasMore(listResult.headers.link as string | undefined);

  return {
    data: {
      type: 'issues',
      owner,
      repo,
      issues: params.concise
        ? issues.map(i => `#${i.number} ${i.title}`)
        : issues,
      total_count: issues.length,
      pagination: {
        currentPage,
        perPage,
        hasMore,
        ...(hasMore ? { nextPage: currentPage + 1 } : {}),
      },
    },
    status: 200,
  };
}
