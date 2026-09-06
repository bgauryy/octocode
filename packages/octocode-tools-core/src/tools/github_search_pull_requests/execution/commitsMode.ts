import type { AuthInfo } from '@modelcontextprotocol/server';
import { GITHUB_SEARCH_HISTORY_TOOL_NAME } from '../../toolNames.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchHistory } from '../../../github/history.js';
import { searchCommits } from '../../../github/commitSearch.js';
import { compareRefs } from '../../../github/compare.js';
import { withDiffContinuations } from '../historyDiffContinuations.js';
import { isGitHubAPIError } from '../../../github/githubAPI.js';
import type { ProcessedBulkResult } from '../../../types/toolResults.js';
import type {
  GitHubPullRequestSearchInput,
  GitHubPullRequestSearchQuery,
} from './types.js';

// --- commits mode: route to commit history API ---
export async function handleCommitsMode(
  query: GitHubPullRequestSearchInput,
  parsedData: GitHubPullRequestSearchQuery | undefined,
  authInfo: AuthInfo | undefined,
  toolName = GITHUB_SEARCH_HISTORY_TOOL_NAME
): Promise<ProcessedBulkResult> {
  const q = parsedData as {
    type?: string;
    owner?: string;
    repo?: string;
    keywords?: string[];
    path?: string;
    branch?: string;
    author?: string;
    committer?: string;
    base?: string;
    head?: string;
    since?: string;
    until?: string;
    page?: number;
    filePage?: number;
    pageSize?: number;
    includeDiff?: boolean;
    charOffset?: number;
    charLength?: number;
  };

  if (!q.owner || !q.repo) {
    return createErrorResult(
      'owner and repo are required for commits mode.',
      query
    );
  }

  if (q.keywords?.length) {
    if (
      q.path !== undefined ||
      q.branch !== undefined ||
      q.base ||
      q.head ||
      q.includeDiff
    ) {
      return createErrorResult(
        'Commit-message keywords cannot be combined with path, branch, compare, or diffs; search covers the default branch.',
        query
      );
    }
    const result = await searchCommits(
      {
        owner: q.owner,
        repo: q.repo,
        keywords: q.keywords,
        author: q.author,
        committer: q.committer,
        since: q.since,
        until: q.until,
        page: q.page ?? 1,
        perPage: q.pageSize ?? 30,
      },
      authInfo
    );
    if (isGitHubAPIError(result))
      return createErrorResult(result, query, { toolName });
    return createSuccessResult(
      query,
      result.data,
      result.data.commits.length > 0,
      toolName,
      { rawResponse: result.rawResponseChars }
    );
  }

  // Compare mode: base+head diffs two refs instead of walking history.
  if (q.base && q.head) {
    const compare = await compareRefs(
      {
        owner: q.owner,
        repo: q.repo,
        base: q.base,
        head: q.head,
        page: q.page,
        includeDiff: Boolean(q.includeDiff),
        // Scope + paginate the diff exactly like the history-walk path so a
        // large commit is searchable-by-path and windowed, not one big dump.
        path: q.path,
        filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
        itemsPerPage: q.pageSize,
        charOffset: typeof q.charOffset === 'number' ? q.charOffset : undefined,
        charLength: typeof q.charLength === 'number' ? q.charLength : undefined,
      },
      authInfo
    );
    if (isGitHubAPIError(compare)) {
      return createErrorResult(compare, query, {
        toolName,
      });
    }
    const compareData = compare.data as unknown as Record<string, unknown>;
    const publicQuery = {
      operation: 'compare',
      owner: q.owner,
      repo: q.repo,
      base: q.base,
      head: q.head,
      ...(q.page === undefined ? {} : { page: q.page }),
      ...(q.includeDiff ? { includeDiff: true } : {}),
      ...(q.path ? { path: q.path } : {}),
      ...(q.filePage === undefined ? {} : { filePage: q.filePage }),
      ...(q.pageSize === undefined ? {} : { pageSize: q.pageSize }),
      ...(q.charOffset === undefined ? {} : { charOffset: q.charOffset }),
      ...(q.charLength === undefined ? {} : { charLength: q.charLength }),
    };
    return createSuccessResult(
      query,
      withDiffContinuations(compareData, publicQuery),
      compare.data.totalCommits > 0 || compare.data.status !== 'identical',
      toolName
    );
  }

  const path = q.path;
  // A path ending in '/' is a directory prefix → repo mode; a specific file path → file mode
  const historyType = path && !path.endsWith('/') ? 'file' : 'repo';

  if (historyType === 'file' && !path) {
    return createErrorResult(
      'path is required when querying a specific file in commits mode.',
      query
    );
  }

  const result = await fetchHistory(
    {
      type: historyType,
      owner: q.owner,
      repo: q.repo,
      path,
      branch: q.branch,
      since: q.since,
      until: q.until,
      author: q.author,
      committer: q.committer,
      page: Number(q.page) || 1,
      perPage: Number(q.pageSize) || 30,
      filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
      itemsPerPage: typeof q.pageSize === 'number' ? q.pageSize : undefined,
      includeDiff: Boolean(q.includeDiff),
      charOffset: typeof q.charOffset === 'number' ? q.charOffset : undefined,
      charLength: typeof q.charLength === 'number' ? q.charLength : undefined,
    },
    authInfo
  );

  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName,
    });
  }

  const { commits } = result.data;
  const hasContent = commits.length > 0;

  // Commit headlines reference their PR as "(#N)" — hand the agent a
  // ready-made PR lookup for the first referenced PR instead of
  // making it build the exact-item call manually.
  const prRef = commits
    .map(c => c.messageHeadline?.match(/\(#(\d+)\)/)?.[1])
    .find(Boolean);
  // On an EMPTY walk the explanation IS the payload: fetchHistory's
  // warnings (e.g. "since/until filter by committer date") would be stripped
  // by the no-warnings egress contract, so hoist them onto the hints channel.
  const emptyWalkHints =
    !hasContent && result.data.warnings?.length
      ? { hints: result.data.warnings }
      : {};

  const dataWithNext = {
    ...(result.data as unknown as Record<string, unknown>),
    ...emptyWalkHints,
    ...(prRef
      ? {
          next: {
            prDetail: {
              tool: 'ghGetHistoryItem',
              query: {
                operation: 'pullRequest',
                owner: q.owner,
                repo: q.repo,
                number: Number(prRef),
              },
              why: `Open PR #${prRef} referenced by the first commit for review context`,
              confidence: 'low',
            },
          },
        }
      : {}),
  };

  return createSuccessResult(query, dataWithNext, hasContent, toolName, {
    rawResponse: result.rawResponseChars,
  });
}
// --- end commits mode ---
