/**
 * Bitbucket Pull Request Search
 *
 * Search and retrieve pull requests from Bitbucket repositories.
 *
 * @module bitbucket/pullRequestSearch
 */

import { getBitbucketClient } from './client.js';
import { getBitbucketHost } from '../bitbucketConfig.js';
import { getAuthHeader } from './client.js';
import { handleBitbucketAPIError, createBitbucketError } from './errors.js';
import type {
  BitbucketAPIResponse,
  BitbucketPaginatedResponse,
  BitbucketPullRequest,
  BitbucketPRComment,
  BitbucketDiffstatEntry,
} from './types.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  isBitbucketDiffstatEntry,
  isBitbucketPullRequest,
  parseBitbucketPaginatedResponse,
} from './responseGuards.js';

interface BitbucketPRSearchQuery {
  workspace: string;
  repoSlug: string;
  prNumber?: number;
  state?: 'OPEN' | 'MERGED' | 'DECLINED' | 'SUPERSEDED';
  author?: string;
  baseBranch?: string;
  headBranch?: string;
  sort?: string;
  page?: number;
  limit?: number;
  withComments?: boolean;
  withDiff?: boolean;
  withDiffstat?: boolean;
}

interface BitbucketPRSearchResult {
  pullRequests: BitbucketPullRequest[];
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    totalMatches?: number;
  };
  comments?: BitbucketPRComment[];
  diff?: string;
  diffstat?: BitbucketDiffstatEntry[];
}

interface BitbucketPRSupplementalData {
  comments?: BitbucketPRComment[];
  diff?: string;
  diffstat?: BitbucketDiffstatEntry[];
}

function buildPRQueryFilter(
  params: BitbucketPRSearchQuery
): string | undefined {
  const filters: string[] = [];

  if (params.author) {
    filters.push(`author.display_name ~ "${params.author}"`);
  }

  if (params.baseBranch) {
    filters.push(`destination.branch.name = "${params.baseBranch}"`);
  }

  if (params.headBranch) {
    filters.push(`source.branch.name = "${params.headBranch}"`);
  }

  return filters.length > 0 ? filters.join(' AND ') : undefined;
}

export async function searchBitbucketPRsAPI(
  params: BitbucketPRSearchQuery
): Promise<BitbucketAPIResponse<BitbucketPRSearchResult>> {
  if (!params.workspace || !params.repoSlug) {
    return createBitbucketError(
      'Workspace and repo slug are required for PR search.',
      400
    );
  }

  const cacheKey = generateCacheKey('bb-api-prs', params);
  return withDataCache(
    cacheKey,
    async () => {
      try {
        // Single PR fetch
        if (params.prNumber) {
          return await fetchSinglePR(params);
        }

        // List PRs
        return await listPRs(params);
      } catch (error) {
        return handleBitbucketAPIError(
          error
        ) as BitbucketAPIResponse<BitbucketPRSearchResult>;
      }
    },
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchSinglePR(
  params: BitbucketPRSearchQuery
): Promise<BitbucketAPIResponse<BitbucketPRSearchResult>> {
  const client = getBitbucketClient();

  const { data } = await client.GET(
    '/repositories/{workspace}/{repo_slug}/pullrequests/{pull_request_id}',
    {
      params: {
        path: {
          workspace: params.workspace,
          repo_slug: params.repoSlug,
          pull_request_id: params.prNumber!,
        },
      },
    }
  );

  if (!isBitbucketPullRequest(data)) {
    return createBitbucketError(
      'Unexpected Bitbucket pull request response shape.',
      502
    );
  }

  const pr = data;
  const result: BitbucketPRSearchResult = {
    pullRequests: [pr],
    pagination: {
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      totalMatches: 1,
    },
  };

  if (params.withComments) {
    result.comments = await fetchPRComments(params);
  }

  if (params.withDiff) {
    result.diff = await fetchPRDiff(params);
  }

  if (params.withDiffstat) {
    result.diffstat = await fetchPRDiffstat(params);
  }

  return { data: result, status: 200 };
}

export async function fetchBitbucketPRSupplementalData(
  params: BitbucketPRSearchQuery
): Promise<BitbucketPRSupplementalData> {
  const result: BitbucketPRSupplementalData = {};

  if (params.withComments) {
    result.comments = await fetchPRComments(params);
  }

  if (params.withDiff) {
    result.diff = await fetchPRDiff(params);
  }

  if (params.withDiffstat) {
    result.diffstat = await fetchPRDiffstat(params);
  }

  return result;
}

async function listPRs(
  params: BitbucketPRSearchQuery
): Promise<BitbucketAPIResponse<BitbucketPRSearchResult>> {
  const client = getBitbucketClient();
  const q = buildPRQueryFilter(params);

  const queryParams: Record<string, string> = {};
  if (params.state) queryParams.state = params.state;
  if (q) queryParams.q = q;
  if (params.sort)
    queryParams.sort =
      params.sort === 'updated' ? '-updated_on' : '-created_on';
  queryParams.pagelen = String(params.limit || 10);
  queryParams.page = String(params.page || 1);

  const { data } = await client.GET(
    '/repositories/{workspace}/{repo_slug}/pullrequests',
    {
      params: {
        path: {
          workspace: params.workspace,
          repo_slug: params.repoSlug,
        },
        query: queryParams as Record<string, string>,
      },
    }
  );

  const paginated = parseBitbucketPaginatedResponse(
    data,
    isBitbucketPullRequest
  );
  if (!paginated) {
    return createBitbucketError(
      'Unexpected Bitbucket pull request list response shape.',
      502
    );
  }
  const prs = paginated.values;
  const size = paginated?.size || prs.length;
  const pagelen = params.limit || 10;

  return {
    data: {
      pullRequests: prs,
      pagination: {
        currentPage: paginated?.page || params.page || 1,
        totalPages: Math.ceil(size / pagelen),
        hasMore: !!paginated?.next,
        totalMatches: size,
      },
    },
    status: 200,
  };
}

async function fetchPRComments(
  params: BitbucketPRSearchQuery
): Promise<BitbucketPRComment[]> {
  try {
    const host = getBitbucketHost();
    const authHeader = getAuthHeader();
    const url = `${host}/repositories/${encodeURIComponent(params.workspace)}/${encodeURIComponent(params.repoSlug)}/pullrequests/${params.prNumber}/comments`;
    return await fetchPaginatedCollection<BitbucketPRComment>(url, {
      Authorization: authHeader,
      Accept: 'application/json',
    });
  } catch (error) {
    // Re-throw rate limit errors so they bubble up to the main handler
    if (
      error instanceof Error &&
      'status' in error &&
      (error as { status: number }).status === 429
    ) {
      throw error;
    }
    return [];
  }
}

async function fetchPRDiff(
  params: BitbucketPRSearchQuery
): Promise<string | undefined> {
  try {
    const host = getBitbucketHost();
    const authHeader = getAuthHeader();
    const url = `${host}/repositories/${encodeURIComponent(params.workspace)}/${encodeURIComponent(params.repoSlug)}/pullrequests/${params.prNumber}/diff`;

    const response = await fetch(url, {
      headers: { Authorization: authHeader },
    });

    if (!response.ok) {
      if (response.status === 429) {
        throw Object.assign(new Error('Rate limited'), {
          status: 429,
          response,
        });
      }
      return undefined;
    }
    return await response.text();
  } catch (error) {
    if (
      error instanceof Error &&
      'status' in error &&
      (error as { status: number }).status === 429
    ) {
      throw error;
    }
    return undefined;
  }
}

async function fetchPRDiffstat(
  params: BitbucketPRSearchQuery
): Promise<BitbucketDiffstatEntry[]> {
  try {
    const host = getBitbucketHost();
    const authHeader = getAuthHeader();
    const url = `${host}/repositories/${encodeURIComponent(params.workspace)}/${encodeURIComponent(params.repoSlug)}/pullrequests/${params.prNumber}/diffstat`;
    return await fetchPaginatedCollection(
      url,
      {
        Authorization: authHeader,
        Accept: 'application/json',
      },
      isBitbucketDiffstatEntry
    );
  } catch (error) {
    if (
      error instanceof Error &&
      'status' in error &&
      (error as { status: number }).status === 429
    ) {
      throw error;
    }
    return [];
  }
}

async function fetchPaginatedCollection<T>(
  initialUrl: string,
  headers: Record<string, string>,
  itemGuard?: (item: unknown) => item is T
): Promise<T[]> {
  const items: T[] = [];
  let nextUrl: string | undefined = initialUrl;

  while (nextUrl) {
    const response: Response = await fetch(nextUrl, { headers });

    if (!response.ok) {
      if (response.status === 429) {
        throw Object.assign(new Error('Rate limited'), {
          status: 429,
          response,
        });
      }

      return [];
    }

    const paginated: BitbucketPaginatedResponse<T> | null =
      parseBitbucketPaginatedResponse(
        await response.json(),
        itemGuard ??
          ((item: unknown): item is T => item !== null && item !== undefined)
      );
    if (!paginated) break;
    items.push(...paginated.values);
    nextUrl = paginated.next;
  }

  return items;
}
