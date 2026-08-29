import type { AuthInfo } from '@modelcontextprotocol/server';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchHistory } from '../../../github/history.js';
import { compareRefs } from '../../../github/compare.js';
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
    committer?: string;
    base?: string;
    head?: string;
    since?: string;
    until?: string;
    page?: number;
    filePage?: number;
    itemsPerPage?: number;
    limit?: number;
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

  // Compare mode: base+head diffs two refs instead of walking history.
  if (q.base && q.head) {
    const compareRawLimit = (query as { limit?: unknown }).limit;
    const compare = await compareRefs(
      {
        owner: q.owner,
        repo: q.repo,
        base: q.base,
        head: q.head,
        includeDiff: Boolean(q.includeDiff),
        // Scope + paginate the diff exactly like the history-walk path so a
        // large commit is searchable-by-path and windowed, not one big dump.
        path: q.path,
        filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
        itemsPerPage:
          typeof compareRawLimit === 'number'
            ? compareRawLimit
            : q.itemsPerPage,
        charOffset: typeof q.charOffset === 'number' ? q.charOffset : undefined,
        charLength: typeof q.charLength === 'number' ? q.charLength : undefined,
      },
      authInfo
    );
    if (isGitHubAPIError(compare)) {
      return createErrorResult(compare, query, {
        toolName: TOOL_NAMES.GITHUB_COMMITS,
      });
    }
    return createSuccessResult(
      query,
      compare.data as unknown as Record<string, unknown>,
      compare.data.totalCommits > 0 || compare.data.status !== 'identical',
      TOOL_NAMES.GITHUB_COMMITS
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

  // `limit` is an alias for the commits-per-page size; prefer it only when it is
  // explicitly present in the raw query. parsedData may carry a defaulted `limit`
  // from the unified PR schema, which must not override an explicit itemsPerPage.
  const rawLimit = (query as { limit?: unknown }).limit;
  const effectivePerPage =
    typeof rawLimit === 'number' ? rawLimit : q.itemsPerPage;

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
      // itemsPerPage is the agent-facing commits-per-page field; it feeds the
      // GitHub per_page for the commit list.
      perPage: Number(effectivePerPage) || 30,
      filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
      itemsPerPage:
        typeof effectivePerPage === 'number' ? effectivePerPage : undefined,
      includeDiff: Boolean(q.includeDiff),
      charOffset: typeof q.charOffset === 'number' ? q.charOffset : undefined,
      charLength: typeof q.charLength === 'number' ? q.charLength : undefined,
    },
    authInfo
  );

  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName: TOOL_NAMES.GITHUB_COMMITS,
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
              tool: 'ghSearchPullRequests',
              query: {
                owner: q.owner,
                repo: q.repo,
                prNumber: Number(prRef),
              },
              why: `Open PR #${prRef} referenced by the first commit for review context`,
              confidence: 'low',
            },
          },
        }
      : {}),
  };

  return createSuccessResult(
    query,
    dataWithNext,
    hasContent,
    TOOL_NAMES.GITHUB_COMMITS,
    {
      rawResponse: result.rawResponseChars,
    }
  );
}
// --- end commits mode ---
