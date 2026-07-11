import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from './errors.js';
import { parseHasMore } from './history.js';
import type { GitHubAPIError, GitHubAPIResponse } from './githubAPI.js';
import {
  buildIssueSearchQuery,
  shouldUseSearchForIssues,
  type IssueSearchParams,
} from './queryBuilders.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../config.js';

export type IssueCommentRow = {
  id: string;
  user: string;
  body: string;
  created_at: string;
  updated_at: string;
  commentType: 'discussion';
};

export type IssueRow = {
  number: number;
  title: string;
  state: string;
  author: string;
  labels: string[];
  created_at: string;
  updated_at: string;
  closed_at?: string;
  url: string;
  body?: string;
  comments?: IssueCommentRow[];
  contentPagination?: {
    body?: {
      charOffset: number;
      charLength: number;
      totalChars: number;
      hasMore: boolean;
      nextCharOffset?: number;
    };
    comments?: {
      currentPage: number;
      itemsPerPage: number;
      totalComments: number;
      hasMore: boolean;
      nextCommentPage?: number;
    };
  };
};

export type IssuesResult = {
  type: 'issues';
  owner: string;
  repo: string;
  issues: IssueRow[] | string[];
  total_count?: number;
  effectiveQuery?: string;
  incomplete_results?: boolean;
  pagination?: {
    currentPage: number;
    perPage: number;
    hasMore: boolean;
    nextPage?: number;
    totalMatches?: number;
    reportedTotalMatches?: number;
    totalMatchesKind?: 'reported';
  };
};

export type FetchIssuesParams = IssueSearchParams & {
  issueNumber?: number;
  keywordsToSearch?: string[];
  concise?: boolean;
  content?: {
    body?: boolean;
    comments?: {
      discussion?: boolean;
      includeBots?: boolean;
    };
  };
  charOffset?: number;
  charLength?: number;
  commentPage?: number;
  itemsPerPage?: number;
};

const KNOWN_BOT_LOGINS = new Set([
  'vercel',
  'pkg-pr-new',
  'coderabbitai',
  'github-actions',
  'codecov',
  'changeset-bot',
  'netlify',
  'sonarcloud',
  'socket-security',
]);

function isBotAuthor(login: string): boolean {
  const lower = login.toLowerCase();
  return lower.endsWith('[bot]') || KNOWN_BOT_LOGINS.has(lower);
}

function quoteKeyword(kw: string): string {
  if (kw.startsWith('"')) return kw;
  if (/\s/.test(kw)) return `"${kw.replace(/"/g, '\\"')}"`;
  return kw;
}

function firstString(
  value: string | string[] | undefined
): string | undefined {
  if (typeof value === 'string') return value;
  if (Array.isArray(value) && typeof value[0] === 'string') return value[0];
  return undefined;
}

function combineQuery(params: FetchIssuesParams): string | undefined {
  const keywordParts = (params.keywordsToSearch ?? [])
    .filter(k => k.trim())
    .map(quoteKeyword);
  const rawQuery = params.query?.trim() ?? '';
  const combined = [...keywordParts, ...(rawQuery ? [rawQuery] : [])].join(' ');
  return combined || undefined;
}

function windowText(
  text: string,
  charOffset: number | undefined,
  charLength: number | undefined
): {
  text: string;
  pagination?: NonNullable<IssueRow['contentPagination']>['body'];
} {
  if (!charLength && !charOffset) return { text };
  const totalChars = text.length;
  const start = Math.min(Math.max(0, charOffset ?? 0), totalChars);
  const length = Math.max(1, charLength ?? totalChars);
  const end = Math.min(start + length, totalChars);
  const hasMore = end < totalChars;
  return {
    text: text.slice(start, end),
    pagination: {
      charOffset: start,
      charLength: end - start,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: end } : {}),
    },
  };
}

function hasPullRequestField(item: {
  pull_request?: unknown;
}): boolean {
  return item.pull_request != null;
}

function mapIssueLabels(
  labels: Array<string | { name?: string | null }> | undefined
): string[] {
  if (!labels) return [];
  return labels
    .map(label => (typeof label === 'string' ? label : label.name ?? ''))
    .filter(Boolean);
}

function toIssueRow(item: {
  number: number;
  title?: string | null;
  state?: string | null;
  user?: { login?: string | null } | null;
  labels?: Array<string | { name?: string | null }>;
  created_at?: string | null;
  updated_at?: string | null;
  closed_at?: string | null;
  html_url?: string | null;
  body?: string | null;
}): IssueRow {
  return {
    number: item.number,
    title: item.title ?? '',
    state: item.state ?? 'open',
    author: item.user?.login ?? 'unknown',
    labels: mapIssueLabels(item.labels),
    created_at: item.created_at ?? '',
    updated_at: item.updated_at ?? '',
    ...(item.closed_at ? { closed_at: item.closed_at } : {}),
    url: item.html_url ?? '',
  };
}

function createIssueError(
  message: string,
  hints: string[] = []
): GitHubAPIError {
  return {
    error: message,
    type: 'http',
    ...(hints.length > 0 ? { hints } : {}),
  };
}

export function buildIssueSearchCacheKey(
  params: FetchIssuesParams,
  sessionId?: string,
  authFingerprint: string = 'anon'
): string {
  return generateCacheKey(
    'gh-api-issues',
    {
      owner: params.owner,
      repo: params.repo,
      issueNumber: params.issueNumber,
      query: combineQuery(params),
      state: params.state,
      author: params.author,
      assignee: params.assignee,
      mentions: params.mentions,
      commenter: params.commenter,
      involves: params.involves,
      label: params.label,
      milestone: params.milestone,
      created: params.created,
      updated: params.updated,
      closed: params.closed,
      comments: params.comments,
      reactions: params.reactions,
      interactions: params.interactions,
      locked: params.locked,
      visibility: params.visibility,
      archived: params.archived,
      'no-assignee': params['no-assignee'],
      'no-label': params['no-label'],
      'no-milestone': params['no-milestone'],
      'no-project': params['no-project'],
      match: params.match,
      sort: params.sort,
      order: params.order,
      limit: params.limit,
      page: params.page,
      content: params.content,
      charOffset: params.charOffset,
      charLength: params.charLength,
      commentPage: params.commentPage,
      itemsPerPage: params.itemsPerPage,
      concise: params.concise,
      auth: authFingerprint,
    },
    sessionId
  );
}

export async function fetchIssues(
  params: FetchIssuesParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<IssuesResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = buildIssueSearchCacheKey(params, sessionId, auth);
  return withDataCache<GitHubAPIResponse<IssuesResult>>(
    cacheKey,
    () => fetchIssuesInternal(params, authInfo),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchIssuesInternal(
  params: FetchIssuesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<IssuesResult>> {
  try {
    if (params.state === 'merged') {
      return createIssueError(
        'state:"merged" is not valid for type:"issues" — use "open" or "closed".',
        ['For merged PRs use type:"prs" with state:"merged".']
      );
    }

    if (params.issueNumber != null) {
      return await fetchIssueByNumber(params, authInfo);
    }

    const searchParams: IssueSearchParams = {
      ...params,
      query: combineQuery(params),
    };

    if (shouldUseSearchForIssues(searchParams)) {
      return await searchIssues(searchParams, params, authInfo);
    }

    return await listIssues(params, authInfo);
  } catch (error) {
    if (isNoResultsSearchError(error)) {
      return {
        data: {
          type: 'issues',
          owner: firstString(params.owner) ?? '',
          repo: firstString(params.repo) ?? '',
          issues: [],
          total_count: 0,
          pagination: {
            currentPage: params.page ?? 1,
            perPage: Math.min(
              params.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
              GITHUB_SEARCH_MAX_LIMIT
            ),
            hasMore: false,
          },
        },
        status: 200,
      };
    }
    return handleGitHubAPIError(error);
  }
}

async function fetchIssueByNumber(
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

async function searchIssues(
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
      | 'comments'
      | 'reactions'
      | 'created'
      | 'updated'
      | undefined,
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

async function listIssues(
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
