/**
 * Bitbucket Repository Structure
 *
 * Browse repository tree structure on Bitbucket.
 * Uses the same /src/{commit}/{path} endpoint with format=meta for directory listing.
 *
 * @module bitbucket/repoStructure
 */

import { getBitbucketClient } from './client.js';
import { getBitbucketDefaultBranch } from './fileContent.js';
import { handleBitbucketAPIError, createBitbucketError } from './errors.js';
import type { BitbucketAPIResponse, BitbucketTreeEntry } from './types.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  isBitbucketTreeEntry,
  parseBitbucketPaginatedResponse,
} from './responseGuards.js';

interface BitbucketRepoStructureQuery {
  workspace: string;
  repoSlug: string;
  ref?: string;
  path?: string;
  depth?: number;
  entriesPerPage?: number;
  entryPageNumber?: number;
}

interface BitbucketRepoStructureResult {
  entries: BitbucketTreeEntry[];
  branch: string;
  path: string;
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    totalMatches?: number;
  };
}

export async function viewBitbucketRepoStructureAPI(
  params: BitbucketRepoStructureQuery
): Promise<BitbucketAPIResponse<BitbucketRepoStructureResult>> {
  if (!params.workspace || !params.repoSlug) {
    return createBitbucketError('Workspace and repo slug are required.', 400);
  }

  const cacheKey = generateCacheKey('bb-repo-structure-api', params);
  return withDataCache(
    cacheKey,
    async () => {
      try {
        const branch =
          params.ref ||
          (await getBitbucketDefaultBranch(params.workspace, params.repoSlug));

        const client = getBitbucketClient();
        const dirPath = params.path || '';
        const pagelen = params.entriesPerPage || 50;
        const pageNum = params.entryPageNumber || 1;

        const queryParams: Record<string, string> = {
          format: 'meta',
          pagelen: String(pagelen),
          page: String(pageNum),
        };
        if (params.depth) {
          queryParams.max_depth = String(params.depth);
        }

        const { data } = await client.GET(
          '/repositories/{workspace}/{repo_slug}/src/{commit}/{path}',
          {
            params: {
              path: {
                workspace: params.workspace,
                repo_slug: params.repoSlug,
                commit: branch,
                path: dirPath,
              },
              query: queryParams,
            },
          }
        );
        const rawData = parseBitbucketPaginatedResponse(
          data,
          isBitbucketTreeEntry
        );
        if (!rawData) {
          return createBitbucketError(
            'Unexpected Bitbucket repository tree response shape.',
            502
          );
        }

        const entries = rawData.values;
        const size = rawData.size || entries.length;

        return {
          data: {
            entries,
            branch,
            path: dirPath,
            pagination: {
              currentPage: rawData?.page || pageNum,
              totalPages: Math.ceil(size / pagelen),
              hasMore: !!rawData?.next,
              totalMatches: size,
            },
          },
          status: 200,
        };
      } catch (error) {
        return handleBitbucketAPIError(
          error
        ) as BitbucketAPIResponse<BitbucketRepoStructureResult>;
      }
    },
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}
