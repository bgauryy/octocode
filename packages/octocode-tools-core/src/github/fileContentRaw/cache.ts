import type { AuthInfo } from '@modelcontextprotocol/server';
import type { FileContentExecutionQuery } from '../../tools/github_fetch_content/types.js';
import type { GitHubAPIResponse } from '../githubAPI.js';
import { resolveCacheAuthFingerprint } from '../client.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCacheConditional } from '../../utils/http/cache/conditional.js';
import { fetchRawGitHubFileContent, type RawContentResult } from './fetch.js';

/** Share one authenticated, ref-scoped raw response between reads and materialization. */
export async function fetchCachedRawGitHubFileContent(
  params: FileContentExecutionQuery,
  authInfo?: AuthInfo,
  sessionId?: string
) {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-file-content',
    {
      owner: params.owner,
      repo: params.repo,
      path: params.path,
      branch: params.branch,
      auth,
    },
    sessionId
  );

  const rawResult = await withDataCacheConditional<
    GitHubAPIResponse<RawContentResult>
  >(
    cacheKey,
    async ({ ifNoneMatch }) => {
      const response = await fetchRawGitHubFileContent(params, authInfo, {
        ifNoneMatch,
      });
      const { etag, notModified, ...value } = response;
      return {
        value: value as GitHubAPIResponse<RawContentResult>,
        etag,
        notModified,
      };
    },
    {
      shouldCache: (value: GitHubAPIResponse<RawContentResult>) =>
        'data' in value && !(value as { error?: unknown }).error,
      forceRefresh: params.forceRefresh === true,
    }
  );

  return { rawResult, auth };
}
