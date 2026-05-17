/**
 * Bitbucket Code Search
 *
 * Search for code within a Bitbucket workspace.
 * Bitbucket scopes code search to workspace level, not individual repos.
 *
 * @module bitbucket/codeSearch
 */

import { getBitbucketClient } from './client.js';
import { handleBitbucketAPIError, createBitbucketError } from './errors.js';
import type {
  BitbucketAPIResponse,
  BitbucketCodeSearchResult,
  BitbucketCodeSearchItem,
} from './types.js';
import { getBitbucketRepositoryIdentity } from './searchUtils.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { parseBitbucketCodeSearchPage } from './responseGuards.js';

interface BitbucketCodeSearchQuery {
  workspace: string;
  repoSlug?: string;
  searchQuery: string;
  path?: string;
  filename?: string;
  extension?: string;
  page?: number;
  limit?: number;
}

function quoteSearchModifier(
  value: string,
  forceQuote: boolean = false
): string {
  return forceQuote || /[\s"]/u.test(value)
    ? `"${value.replaceAll('"', '\\"')}"`
    : value;
}

function buildSearchQuery(params: BitbucketCodeSearchQuery): string {
  const queryParts = [params.searchQuery.trim()];

  if (params.repoSlug) {
    queryParts.push(`repo:${quoteSearchModifier(params.repoSlug)}`);
  }

  if (params.path) {
    queryParts.push(`path:${quoteSearchModifier(params.path, true)}`);
  }

  return queryParts.join(' ');
}

function filterSearchResults(
  items: BitbucketCodeSearchItem[],
  params: BitbucketCodeSearchQuery
): BitbucketCodeSearchItem[] {
  return items.filter(item => {
    const filePath = item.file?.path || '';
    const fileName = filePath.split('/').pop() || '';
    const repositoryIdentity = getBitbucketRepositoryIdentity(item);

    if (
      params.repoSlug &&
      repositoryIdentity &&
      repositoryIdentity.repoSlug !== params.repoSlug
    ) {
      return false;
    }

    if (params.path && !filePath.startsWith(params.path)) {
      return false;
    }

    if (params.filename && fileName !== params.filename) {
      return false;
    }

    if (params.extension && !fileName.endsWith(`.${params.extension}`)) {
      return false;
    }

    return true;
  });
}

export async function searchBitbucketCodeAPI(
  params: BitbucketCodeSearchQuery
): Promise<BitbucketAPIResponse<BitbucketCodeSearchResult>> {
  if (!params.workspace) {
    return createBitbucketError(
      'Workspace is required for Bitbucket code search. Provide a projectId in the format "workspace/repo_slug".',
      400,
      [
        'Bitbucket code search is scoped to a workspace.',
        'Provide owner parameter as "workspace" or projectId as "workspace/repo".',
      ]
    );
  }

  if (!params.searchQuery?.trim()) {
    return createBitbucketError('Search query is required.', 400);
  }

  const cacheKey = generateCacheKey('bb-api-code', params);
  return withDataCache(
    cacheKey,
    async () => {
      try {
        const client = getBitbucketClient();

        const queryParams = {
          search_query: buildSearchQuery(params),
          page: params.page || 1,
          pagelen: params.limit || 20,
        };
        const { data } = await client.GET(
          '/workspaces/{workspace}/search/code',
          {
            params: {
              path: { workspace: params.workspace },
              query: queryParams as typeof queryParams & Record<string, string>,
            },
          }
        );
        const pageData = parseBitbucketCodeSearchPage(data);
        if (!pageData) {
          return createBitbucketError(
            'Unexpected Bitbucket code search response shape.',
            502
          );
        }

        const pagelen = params.limit || 20;
        const items = filterSearchResults(pageData.values, params);

        return {
          data: {
            items,
            totalCount: pageData.size,
            pagination: {
              currentPage: pageData.page,
              totalPages: Math.ceil(pageData.size / pagelen),
              hasMore: !!pageData.next,
              totalMatches: pageData.size,
            },
          },
          status: 200,
        };
      } catch (error) {
        return handleBitbucketAPIError(
          error
        ) as BitbucketAPIResponse<BitbucketCodeSearchResult>;
      }
    },
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}
