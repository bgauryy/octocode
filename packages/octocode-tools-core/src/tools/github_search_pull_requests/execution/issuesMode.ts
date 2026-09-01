import type { AuthInfo } from '@modelcontextprotocol/server';
import { GITHUB_SEARCH_HISTORY_TOOL_NAME } from '../../toolNames.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchIssues } from '../../../github/issues.js';
import { isGitHubAPIError } from '../../../github/githubAPI.js';
import type { ProcessedBulkResult } from '../../../types/toolResults.js';
import type {
  GitHubPullRequestSearchInput,
  GitHubPullRequestSearchQuery,
} from './types.js';

// --- issues mode: search/list/read GitHub issues (not PRs) ---
export async function handleIssuesMode(
  query: GitHubPullRequestSearchInput,
  parsedData: GitHubPullRequestSearchQuery | undefined,
  authInfo: AuthInfo | undefined,
  toolName = GITHUB_SEARCH_HISTORY_TOOL_NAME
): Promise<ProcessedBulkResult> {
  const q = parsedData as {
    owner?: string;
    repo?: string;
    issueNumber?: number;
    prNumber?: number;
    keywords?: string[];
    query?: string;
    state?: 'open' | 'closed' | 'merged';
    author?: string;
    assignee?: string;
    mentions?: string;
    commenter?: string;
    label?: string | string[];
    milestone?: string;
    created?: string;
    updated?: string;
    closed?: string;
    comments?: number | string;
    reactions?: number | string;
    locked?: boolean;
    visibility?: 'public' | 'private';
    match?: ('title' | 'body' | 'comments')[];
    archived?: boolean;
    sort?: 'created' | 'updated' | 'best-match' | 'comments' | 'reactions';
    order?: 'asc' | 'desc';
    pageSize?: number;
    page?: number;
    concise?: boolean;
    content?: {
      body?: boolean;
      comments?: { discussion?: boolean; includeBots?: boolean };
    };
    charOffset?: number;
    charLength?: number;
    commentPage?: number;
  };
  if (!q.owner || !q.repo) {
    return createErrorResult(
      'owner and repo are required for issues mode.',
      query
    );
  }
  const issueNumber = q.issueNumber ?? q.prNumber;
  const result = await fetchIssues(
    {
      owner: q.owner,
      repo: q.repo,
      ...(issueNumber != null ? { issueNumber } : {}),
      keywordsToSearch: q.keywords,
      query: q.query,
      state: q.state,
      author: q.author,
      assignee: q.assignee,
      mentions: q.mentions,
      commenter: q.commenter,
      label: q.label,
      milestone: q.milestone,
      created: q.created,
      updated: q.updated,
      closed: q.closed,
      comments: q.comments,
      reactions: q.reactions,
      locked: q.locked,
      visibility: q.visibility,
      match: q.match,
      archived: q.archived,
      sort: q.sort,
      order: q.order,
      limit: q.pageSize,
      page: Number(q.page) || 1,
      concise: q.concise,
      content: q.content,
      charOffset: q.charOffset,
      charLength: q.charLength,
      commentPage: q.commentPage,
      itemsPerPage: q.pageSize,
    },
    authInfo
  );
  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName,
    });
  }
  if (issueNumber != null && Array.isArray(result.data.issues)) {
    const row = result.data.issues[0];
    if (row && typeof row === 'object' && row.contentPagination) {
      const pagination = row.contentPagination as Record<
        string,
        Record<string, unknown>
      >;
      const baseQuery = {
        operation: 'issue',
        owner: q.owner,
        repo: q.repo,
        number: issueNumber,
        content: q.content,
        ...(q.commentPage === undefined ? {} : { commentPage: q.commentPage }),
        ...(q.charLength === undefined ? {} : { charLength: q.charLength }),
      };
      const body = pagination.body;
      if (typeof body?.nextCharOffset === 'number') {
        body.nextQuery = {
          ...baseQuery,
          charOffset: body.nextCharOffset,
        };
      }
      const comments = pagination.comments;
      if (typeof comments?.nextCommentPage === 'number') {
        comments.nextQuery = {
          ...baseQuery,
          commentPage: comments.nextCommentPage,
        };
      }
    }
  }
  const hasContent = Array.isArray(result.data.issues)
    ? result.data.issues.length > 0
    : false;
  // Empty pages can be meaningful: GitHub's issues endpoint also returns PRs,
  // which this tool filters out. The shared egress contract strips `warnings`,
  // so preserve an empty page's explanation as an agent-visible hint.
  const emptyPageHints =
    !hasContent && result.data.warnings?.length
      ? { hints: result.data.warnings }
      : {};

  const issues = result.data.issues;
  const firstIssueNumber = (() => {
    if (!Array.isArray(issues) || issues.length === 0) return undefined;
    const first = issues[0];
    if (typeof first === 'number') return first;
    if (typeof first === 'string') {
      const m = first.match(/^#(\d+)\b/);
      return m ? Number(m[1]) : undefined;
    }
    if (
      first &&
      typeof first === 'object' &&
      typeof (first as { number?: unknown }).number === 'number'
    ) {
      return (first as { number: number }).number;
    }
    return undefined;
  })();

  const next: Record<string, unknown> = {};
  if (issueNumber == null && firstIssueNumber != null) {
    next.readIssue = {
      tool: 'ghGetHistoryItem',
      query: {
        operation: 'issue',
        owner: q.owner,
        repo: q.repo,
        number: firstIssueNumber,
        content: { body: true },
      },
      why: `Read issue #${firstIssueNumber} body/discussion from this list`,
      confidence: 'low',
    };
  }
  next.searchRepositoryCode = {
    tool: 'ghSearch',
    query: {
      operation: 'code',
      owner: q.owner,
      repo: q.repo,
    },
    why: 'Search code in this repository for symbols mentioned in the issue(s)',
    confidence: 'low',
  };

  return createSuccessResult(
    query,
    {
      ...(result.data as unknown as Record<string, unknown>),
      ...emptyPageHints,
      next,
    },
    hasContent,
    toolName,
    { rawResponse: result.rawResponseChars }
  );
}
// --- end issues mode ---
