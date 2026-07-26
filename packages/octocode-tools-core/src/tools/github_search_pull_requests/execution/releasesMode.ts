import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import { fetchReleases } from '../../../github/releases.js';
import { isGitHubAPIError } from '../../../github/githubAPI.js';
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
    itemsPerPage?: number;
    includeAssets?: boolean;
  };
  if (!q.owner || !q.repo) {
    return createErrorResult(
      'owner and repo are required for releases mode.',
      query
    );
  }
  // Releases have no discovery-search, so `itemsPerPage` (the view/page-size
  // field shared with the other modes) is the single knob; it feeds GitHub's
  // per_page for the release list.
  const pageSize = Number(q.itemsPerPage) || RELEASES_PAGE_SIZE_DEFAULT;
  const result = await fetchReleases(
    {
      owner: q.owner,
      repo: q.repo,
      page: Number(q.page) || 1,
      perPage: pageSize,
      includeAssets: q.includeAssets === true,
    },
    authInfo
  );
  if (isGitHubAPIError(result)) {
    return createErrorResult(result, query, {
      toolName: TOOL_NAMES.GITHUB_RELEASES,
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
              tool: 'ghListReleases',
              query: {
                owner: q.owner,
                repo: q.repo,
                page: nextPage,
                itemsPerPage: pageSize,
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
    TOOL_NAMES.GITHUB_RELEASES,
    { rawResponse: result.rawResponseChars }
  );
}
// --- end releases mode ---
