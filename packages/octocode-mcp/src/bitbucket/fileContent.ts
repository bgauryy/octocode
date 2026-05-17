/**
 * Bitbucket File Content
 *
 * Retrieve file contents from Bitbucket repositories.
 * Bitbucket returns raw text for file content (not JSON-wrapped).
 *
 * @module bitbucket/fileContent
 */

import { getBitbucketHost } from '../bitbucketConfig.js';
import { getAuthHeader } from './client.js';
import { handleBitbucketAPIError, createBitbucketError } from './errors.js';
import type {
  BitbucketAPIResponse,
  BitbucketFileContentResult,
} from './types.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { fallbackOnBestEffortFailure } from '../utils/core/bestEffort.js';
import { parseBitbucketDefaultBranch } from './responseGuards.js';

interface BitbucketFileContentQuery {
  workspace: string;
  repoSlug: string;
  path: string;
  ref?: string;
}

export async function fetchBitbucketFileContentAPI(
  params: BitbucketFileContentQuery
): Promise<BitbucketAPIResponse<BitbucketFileContentResult>> {
  if (!params.workspace || !params.repoSlug) {
    return createBitbucketError('Workspace and repo slug are required.', 400);
  }

  if (!params.path) {
    return createBitbucketError('File path is required.', 400);
  }

  const cacheKey = generateCacheKey('bb-api-file-content', params);
  return withDataCache(
    cacheKey,
    async () => {
      const commit =
        params.ref ||
        (await getBitbucketDefaultBranch(params.workspace, params.repoSlug));

      try {
        const host = getBitbucketHost();
        const authHeader = getAuthHeader();
        const encodedPath = params.path
          .split('/')
          .map(segment => encodeURIComponent(segment))
          .join('/');
        const url = `${host}/repositories/${encodeURIComponent(params.workspace)}/${encodeURIComponent(params.repoSlug)}/src/${encodeURIComponent(commit)}/${encodedPath}`;

        const response = await fetch(url, {
          headers: {
            Authorization: authHeader,
          },
        });

        if (!response.ok) {
          const errorBody = await response
            .text()
            .catch(
              fallbackOnBestEffortFailure('bitbucket error body read', '')
            );
          throw Object.assign(
            new Error(errorBody || `HTTP ${response.status}`),
            {
              status: response.status,
            }
          );
        }

        const content = await response.text();

        return {
          data: {
            content,
            path: params.path,
            size: content.length,
            ref: commit,
            encoding: 'utf-8',
          },
          status: 200,
        };
      } catch (error) {
        return handleBitbucketAPIError(
          error
        ) as BitbucketAPIResponse<BitbucketFileContentResult>;
      }
    },
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

/**
 * Resolve the default branch for a Bitbucket repository.
 */
export async function getBitbucketDefaultBranch(
  workspace: string,
  repoSlug: string
): Promise<string> {
  try {
    const host = getBitbucketHost();
    const authHeader = getAuthHeader();
    const url = `${host}/repositories/${encodeURIComponent(workspace)}/${encodeURIComponent(repoSlug)}`;

    const response = await fetch(url, {
      headers: {
        Authorization: authHeader,
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      return 'main';
    }

    const branch = parseBitbucketDefaultBranch(await response.json());

    return branch || 'main';
  } catch {
    return 'main';
  }
}
