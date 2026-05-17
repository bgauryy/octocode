/**
 * Bitbucket Repository Search
 *
 * List and search repositories within a Bitbucket workspace.
 *
 * @module bitbucket/repoSearch
 */

import { getBitbucketClient } from './client.js';
import { handleBitbucketAPIError, createBitbucketError } from './errors.js';
import type { BitbucketAPIResponse, BitbucketRepository } from './types.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  isBitbucketRepository,
  parseBitbucketPaginatedResponse,
} from './responseGuards.js';

interface BitbucketRepoSearchQuery {
  workspace: string;
  keywords?: string[];
  topics?: string[];
  visibility?: string;
  sort?: string;
  page?: number;
  limit?: number;
}

interface BitbucketRepoSearchResult {
  repositories: BitbucketRepository[];
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    totalMatches?: number;
  };
}

function buildQueryFilter(
  params: BitbucketRepoSearchQuery
): string | undefined {
  const filters: string[] = [];

  if (params.keywords?.length) {
    const nameFilter = params.keywords.map(k => `name ~ "${k}"`).join(' AND ');
    filters.push(nameFilter);
  }

  if (params.topics?.length) {
    const topicFilter = params.topics.map(t => `topic = "${t}"`).join(' AND ');
    filters.push(topicFilter);
  }

  if (params.visibility === 'public') {
    filters.push('is_private = false');
  } else if (params.visibility === 'private') {
    filters.push('is_private = true');
  }

  return filters.length > 0 ? filters.join(' AND ') : undefined;
}

function mapSortField(sort?: string): string | undefined {
  const mapping: Record<string, string> = {
    updated: '-updated_on',
    created: '-created_on',
    name: 'name',
  };
  return sort ? mapping[sort] : undefined;
}

export async function searchBitbucketReposAPI(
  params: BitbucketRepoSearchQuery
): Promise<BitbucketAPIResponse<BitbucketRepoSearchResult>> {
  if (!params.workspace) {
    return createBitbucketError(
      'Workspace is required for Bitbucket repository search.',
      400,
      ['Provide owner parameter as the Bitbucket workspace slug.']
    );
  }

  const cacheKey = generateCacheKey('bb-api-repos', params);
  return withDataCache(
    cacheKey,
    async () => {
      try {
        const client = getBitbucketClient();
        const q = buildQueryFilter(params);
        const sort = mapSortField(params.sort);

        const queryParams: Record<string, string> = {};
        if (q) queryParams.q = q;
        if (sort) queryParams.sort = sort;
        queryParams.pagelen = String(params.limit || 10);
        queryParams.page = String(params.page || 1);

        const { data } = await client.GET('/repositories/{workspace}', {
          params: {
            path: { workspace: params.workspace },
            query: queryParams as Record<string, string>,
          },
        });

        const paginated = parseBitbucketPaginatedResponse(
          data,
          isBitbucketRepository
        );
        if (!paginated) {
          return createBitbucketError(
            'Unexpected Bitbucket repository response shape.',
            502
          );
        }
        const repos = paginated.values;
        const size = paginated?.size || repos.length;
        const pagelen = params.limit || 10;

        return {
          data: {
            repositories: repos,
            pagination: {
              currentPage: paginated?.page || params.page || 1,
              totalPages: Math.ceil(size / pagelen),
              hasMore: !!paginated?.next,
              totalMatches: size,
            },
          },
          status: 200,
        };
      } catch (error) {
        return handleBitbucketAPIError(
          error
        ) as BitbucketAPIResponse<BitbucketRepoSearchResult>;
      }
    },
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}
