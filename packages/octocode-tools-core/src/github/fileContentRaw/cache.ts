import type { AuthInfo } from '@modelcontextprotocol/server';
import type { FileContentExecutionQuery } from '../../tools/github_fetch_content/types.js';
import type { GitHubAPIResponse } from '../githubAPI.js';
import { resolveCacheAuthFingerprint } from '../client.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCacheConditional } from '../../utils/http/cache/conditional.js';
import { fetchRawGitHubFileContent, type RawContentResult } from './fetch.js';
import { resolveMaterializationRef } from '../directoryFetch/refResolution.js';

/** Share one authenticated, ref-scoped raw response between reads and materialization. */
export async function fetchCachedRawGitHubFileContent(
  params: FileContentExecutionQuery,
  authInfo?: AuthInfo,
  sessionId?: string
) {
  // Resolve before acquisition: a later lookup could name a different revision
  // from the bytes already fetched. Full SHAs need no provider round trip.
  const ref = params.branch || 'HEAD';
  const branch = /^[a-f0-9]{40}$/i.test(ref)
    ? ref.toLowerCase()
    : (
        await resolveMaterializationRef(
          params.owner,
          params.repo,
          ref,
          authInfo,
          params.forceRefresh === true
        )
      ).commitSha;
  const snapshotQuery = { ...params, branch };
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-file-content',
    {
      owner: params.owner,
      repo: params.repo,
      path: params.path,
      branch,
      auth,
    },
    sessionId
  );

  const rawResult = await withDataCacheConditional<
    GitHubAPIResponse<RawContentResult>
  >(
    cacheKey,
    async ({ ifNoneMatch }) => {
      const response = await fetchRawGitHubFileContent(
        snapshotQuery,
        authInfo,
        {
          ifNoneMatch,
        }
      );
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
