import { quoteSearchKeyword } from '../searchKeyword.js';
import type { GitHubAPIError } from '../githubAPI.js';
import { generateCacheKey } from '../../utils/http/cache.js';
import type { FetchIssuesParams, IssueRow } from './types.js';

export function firstString(
  value: string | string[] | undefined
): string | undefined {
  if (typeof value === 'string') return value;
  if (Array.isArray(value) && typeof value[0] === 'string') return value[0];
  return undefined;
}

export function combineQuery(params: FetchIssuesParams): string | undefined {
  const keywordParts = (params.keywordsToSearch ?? [])
    .filter(k => k.trim())
    .map(quoteSearchKeyword);
  const rawQuery = params.query?.trim() ?? '';
  const combined = [...keywordParts, ...(rawQuery ? [rawQuery] : [])].join(' ');
  return combined || undefined;
}

export function windowText(
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

export function hasPullRequestField(item: { pull_request?: unknown }): boolean {
  return item.pull_request != null;
}

export function mapIssueLabels(
  labels: Array<string | { name?: string | null }> | undefined
): string[] {
  if (!labels) return [];
  return labels
    .map(label => (typeof label === 'string' ? label : (label.name ?? '')))
    .filter(Boolean);
}

export function toIssueRow(item: {
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

export function createIssueError(
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
