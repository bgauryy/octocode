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
  };
  if (!q.owner || !q.repo) {
    return createErrorResult(
      'owner and repo are required for releases mode.',
      query
    );
  }
  const result = await fetchReleases(
    {
      owner: q.owner,
      repo: q.repo,
      page: Number(q.page) || 1,
      perPage: Number(q.perPage) || 30,
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
  return createSuccessResult(
    query,
    result.data as unknown as Record<string, unknown>,
    hasContent,
    TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    { rawResponse: result.rawResponseChars }
  );
}
// --- end releases mode ---
