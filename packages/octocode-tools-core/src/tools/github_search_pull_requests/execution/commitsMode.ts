import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchHistory } from '../../../github/history.js';
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
  authInfo: AuthInfo | undefined
): Promise<ProcessedBulkResult> {
  const q = parsedData as {
    type?: string;
    owner?: string;
    repo?: string;
    path?: string;
    branch?: string;
    author?: string;
    since?: string;
    until?: string;
    page?: number;
    perPage?: number;
    filePage?: number;
    itemsPerPage?: number;
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
      page: Number(q.page) || 1,
      perPage: Number(q.perPage) || 30,
      filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
      itemsPerPage:
        typeof q.itemsPerPage === 'number' ? q.itemsPerPage : undefined,
      includeDiff: Boolean(q.includeDiff),
      charOffset: typeof q.charOffset === 'number' ? q.charOffset : undefined,
      charLength: typeof q.charLength === 'number' ? q.charLength : undefined,
    },
    authInfo
  );

  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    });
  }

  const { commits } = result.data;
  const hasContent = commits.length > 0;

  // Commit headlines reference their PR as "(#N)" — hand the agent a
  // ready-made PR lookup for the first referenced PR instead of
  // making it build the type:"prs" call manually.
  const prRef = commits
    .map(c => c.messageHeadline?.match(/\(#(\d+)\)/)?.[1])
    .find(Boolean);
  const dataWithNext = {
    ...(result.data as unknown as Record<string, unknown>),
    ...(prRef
      ? {
          next: {
            prDetail: {
              tool: 'ghHistoryResearch',
              query: {
                type: 'prs',
                owner: q.owner,
                repo: q.repo,
                prNumber: Number(prRef),
              },
              why: `Open PR #${prRef} referenced by the first commit for review context`,
              confidence: 'heuristic',
            },
          },
        }
      : {}),
  };

  return createSuccessResult(
    query,
    dataWithNext,
    hasContent,
    TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    {
      rawResponse: result.rawResponseChars,
    }
  );
}
// --- end commits mode ---
