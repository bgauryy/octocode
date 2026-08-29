import type { AuthInfo } from '@modelcontextprotocol/server';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { getOctokit } from '../client.js';
import { resolveCanonicalOwnerRepo } from '../canonicalRepo.js';
import { rateLimitWarning } from '../responseHeaders.js';
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
      createdAt: comment.created_at ?? '',
      updatedAt: comment.updated_at ?? '',
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
      totalCount: 1,
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
  // GitHub's Search API does not follow repo renames, so a search scoped to a
  // stale owner/repo silently returns 0 (false absence). Resolve the canonical
  // name and warn, mirroring the PR/code search paths.
  const searchWarnings: string[] = [];
  let effectiveSearchParams = searchParams;
  if (owner && repo) {
    const canonical = await resolveCanonicalOwnerRepo(octokit, owner, repo);
    if (canonical.renamed) {
      searchWarnings.push(
        `Repository ${owner}/${repo} was renamed to ${canonical.owner}/${canonical.repo} — GitHub search does not follow renames, so the canonical name was searched instead.`
      );
      effectiveSearchParams = {
        ...searchParams,
        owner: canonical.owner,
        repo: canonical.repo,
      };
    }
  }
  const q = buildIssueSearchQuery(effectiveSearchParams);
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

  const rateWarn = rateLimitWarning(searchResult.headers);
  if (rateWarn) searchWarnings.push(rateWarn);

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
      totalCount: totalMatches,
      effectiveQuery: q,
      ...(searchWarnings.length ? { warnings: searchWarnings } : {}),
      ...(searchResult.data.incomplete_results
        ? { incompleteResults: true }
        : {}),
      pagination: {
        currentPage,
        perPage,
        hasMore,
        ...(hasMore ? { nextPage: currentPage + 1 } : {}),
        // totalCount (above) is the single count; GitHub search caps the
        // reachable window at 1000, so flag when the real total exceeds it.
        ...(totalMatches > 1000 ? { totalMatchesCapped: true } : {}),
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

  // Pagination honesty: the issues endpoint returns PRs too, filtered out
  // above — so `issues.length` is a page-local, post-filter count, NOT a repo
  // total. Only report it as `totalCount` when this single page is provably
  // the complete result set (no further pages). And when a page filtered
  // down to nothing while more pages exist, say WHY instead of emitting the
  // contradictory-looking `totalCount:0` + `hasMore:true`.
  const isCompleteResultSet = !hasMore && currentPage === 1;
  const warnings: string[] = [];
  if (issues.length === 0 && hasMore) {
    warnings.push(
      'This page contained only pull requests (the GitHub issues endpoint returns both; PRs are filtered out) — follow pagination.nextPage, later pages may contain issues.'
    );
  }

  return {
    data: {
      type: 'issues',
      owner,
      repo,
      issues: params.concise
        ? issues.map(i => `#${i.number} ${i.title}`)
        : issues,
      ...(isCompleteResultSet ? { totalCount: issues.length } : {}),
      ...(warnings.length ? { warnings } : {}),
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
