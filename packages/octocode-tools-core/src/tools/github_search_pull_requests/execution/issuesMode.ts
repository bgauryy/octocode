import type { AuthInfo } from '@modelcontextprotocol/server';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
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
  authInfo: AuthInfo | undefined
): Promise<ProcessedBulkResult> {
  const q = parsedData as {
    owner?: string;
    repo?: string;
    issueNumber?: number;
    prNumber?: number;
    keywordsToSearch?: string[];
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
    limit?: number;
    page?: number;
    concise?: boolean;
    content?: {
      body?: boolean;
      comments?: { discussion?: boolean; includeBots?: boolean };
    };
    charOffset?: number;
    charLength?: number;
    commentPage?: number;
    itemsPerPage?: number;
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
      keywordsToSearch: q.keywordsToSearch,
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
      limit: q.limit,
      page: Number(q.page) || 1,
      concise: q.concise,
      content: q.content,
      charOffset: q.charOffset,
      charLength: q.charLength,
      commentPage: q.commentPage,
      itemsPerPage: q.itemsPerPage,
    },
    authInfo
  );
  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName: TOOL_NAMES.GITHUB_ISSUES,
    });
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
      tool: 'ghSearchIssues',
      query: {
        owner: q.owner,
        repo: q.repo,
        issueNumber: firstIssueNumber,
        content: { body: true },
      },
      why: `Read issue #${firstIssueNumber} body/discussion from this list`,
      confidence: 'low',
    };
  }
  next.searchCode = {
    tool: 'ghSearchCode',
    query: {
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
    TOOL_NAMES.GITHUB_ISSUES,
    { rawResponse: result.rawResponseChars }
  );
}
// --- end issues mode ---
