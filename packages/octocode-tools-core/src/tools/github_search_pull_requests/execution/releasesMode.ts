import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchReleases } from '../../../github/releases.js';
import { isGitHubAPIError } from '../../../github/githubAPI.js';
import { GITHUB_SEARCH_DEFAULT_LIMIT } from '../../../config.js';
import type { ProcessedBulkResult } from '../../../types/toolResults.js';
import type {
  GitHubPullRequestSearchInput,
  GitHubPullRequestSearchQuery,
} from './types.js';

const RELEASES_PAGE_SIZE_DEFAULT = 30;

// --- releases mode: list releases/tags + the repo's latest release ---
export async function handleReleasesMode(
  query: GitHubPullRequestSearchInput,
  parsedData: GitHubPullRequestSearchQuery | undefined,
  authInfo: AuthInfo | undefined
): Promise<ProcessedBulkResult> {
  const q = parsedData as {
    owner?: string;
    repo?: string;
    page?: number;
    perPage?: number;
    limit?: number;
  };
  if (!q.owner || !q.repo) {
    return createErrorResult(
      'owner and repo are required for releases mode.',
      query
    );
  }
  // `perPage` is the mode-specific page-size field, but `limit` is the field
  // shared across every other mode of this tool and the one a caller reaches
  // for first — honor it when perPage was left at its default so `limit`
  // isn't a silent no-op for releases mode. An explicitly-set perPage always
  // wins.
  const effectivePerPage =
    q.perPage !== undefined && q.perPage !== RELEASES_PAGE_SIZE_DEFAULT
      ? q.perPage
      : q.limit !== undefined && q.limit !== GITHUB_SEARCH_DEFAULT_LIMIT
        ? q.limit
        : (q.perPage ?? RELEASES_PAGE_SIZE_DEFAULT);
  const result = await fetchReleases(
    {
      owner: q.owner,
      repo: q.repo,
      page: Number(q.page) || 1,
      perPage: Number(effectivePerPage) || RELEASES_PAGE_SIZE_DEFAULT,
    },
    authInfo
  );
  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    });
  }
  const hasContent =
    result.data.releases.length > 0 || result.data.latest !== undefined;

  // Releases mode used to dead-end with no next-step guidance at all — hand
  // back a ready-made continuation when there's another page, matching the
  // next-hint convention other modes of this tool already use.
  const nextPage = result.data.pagination?.nextPage;
  const dataWithNext = {
    ...(result.data as unknown as Record<string, unknown>),
    ...(nextPage !== undefined
      ? {
          next: {
            nextPage: {
              tool: 'ghHistoryResearch',
              query: {
                type: 'releases',
                owner: q.owner,
                repo: q.repo,
                page: nextPage,
                perPage: effectivePerPage,
              },
              why: 'Fetch the next page of releases',
              confidence: 'exact',
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
    { rawResponse: result.rawResponseChars }
  );
}
// --- end releases mode ---
