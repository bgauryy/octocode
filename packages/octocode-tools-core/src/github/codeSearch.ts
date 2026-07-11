import type {
  SearchCodeParameters,
  CodeSearchResultItem,
  GitHubAPIResponse,
  OptimizedCodeSearchResult,
} from './githubAPI.js';
import type { z } from 'zod';
import type { GitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
import type { WithOptionalMeta } from '../types/execution.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { contextUtils } from '../utils/contextUtils.js';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from './errors.js';
import { buildCodeSearchQuery } from './queryBuilders.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { shouldIgnoreFile } from '../utils/file/filters.js';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { countSerializedChars } from '../utils/response/charSavings.js';
import { normalizeResponseHeaders } from './responseHeaders.js';

import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../config.js';

const RAW_API_DEFAULT_LIMIT = GITHUB_SEARCH_DEFAULT_LIMIT;

export async function searchGitHubCodeAPI(
  params: WithOptionalMeta<GitHubCodeSearchQuery>,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<OptimizedCodeSearchResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-code',
    {
      keywords: params.keywords,
      owner: params.owner,
      repo: params.repo,
      extension: params.extension,
      filename: params.filename,
      language: params.language,
      path: params.path,
      match: params.match,
      limit: params.limit,
      page: params.page,
      auth,
    },
    sessionId
  );

  const result = await withDataCache<
    GitHubAPIResponse<OptimizedCodeSearchResult>
  >(
    cacheKey,
    async () => {
      return await searchGitHubCodeAPIInternal(params, authInfo);
    },
    {
      shouldCache: (value: GitHubAPIResponse<OptimizedCodeSearchResult>) =>
        'data' in value && !(value as { error?: unknown }).error,
    }
  );

  return result;
}

async function searchGitHubCodeAPIInternal(
  params: WithOptionalMeta<GitHubCodeSearchQuery>,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<OptimizedCodeSearchResult>> {
  try {
    const octokit = await getOctokit(authInfo);

    if (params.keywords && params.keywords.length > 0) {
      const validTerms = params.keywords.filter(term => term && term.trim());
      if (validTerms.length === 0) {
        return {
          error: SEARCH_ERRORS.QUERY_EMPTY.message,
          type: 'http',
          status: 400,
        };
      }
    }

    const query = buildCodeSearchQuery(params);

    if (!query.trim()) {
      return {
        error: SEARCH_ERRORS.QUERY_EMPTY.message,
        type: 'http',
        status: 400,
      };
    }

    const perPage = Math.min(
      typeof params.limit === 'number' ? params.limit : RAW_API_DEFAULT_LIMIT,
      GITHUB_SEARCH_MAX_LIMIT
    );
    const currentPage = params.page || 1;

    const searchParams: SearchCodeParameters = {
      q: query,
      per_page: perPage,
      page: currentPage,
      headers: {
        Accept: 'application/vnd.github.v3.text-match+json',
      },
    };

    const result = await octokit.rest.search.code(searchParams);

    const optimizedResult = await transformToOptimizedFormat(
      result.data.items,
      result.data.total_count
    );

    // GitHub sets incomplete_results:true when the code-search index timed out
    // or could not fully complete. The HTTP status is still 200, so without
    // this flag an empty/partial result is indistinguishable from a true
    // no-match. Propagate it so the agent isn't blind to index degradation.
    const incompleteResults = result.data.incomplete_results === true;

    const reportedTotalMatches = optimizedResult.total_count;
    const totalMatches = Math.min(reportedTotalMatches, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;
    const reachableTotalMatches = Math.min(totalMatches, totalPages * perPage);

    return {
      data: {
        total_count: optimizedResult.total_count,
        items: optimizedResult.items,
        ...(incompleteResults ? { incompleteResults: true } : {}),
        repository: optimizedResult.repository,
        matchLocations: optimizedResult.matchLocations,
        minified: optimizedResult.minified,
        minificationFailed: optimizedResult.minificationFailed,
        minificationTypes: optimizedResult.minificationTypes,
        _researchContext: optimizedResult._researchContext,
        pagination: {
          currentPage: clampedPage,
          totalPages,
          perPage,
          totalMatches,
          reportedTotalMatches,
          reachableTotalMatches,
          totalMatchesKind: 'reported',
          totalMatchesCapped: reportedTotalMatches > totalMatches,
          hasMore,
          ...(hasMore ? { nextPage: clampedPage + 1 } : {}),
          uniqueFileCount: optimizedResult._researchContext?.uniqueFileCount,
        },
      },
      status: 200,
      headers: normalizeResponseHeaders(result.headers),
      rawResponseChars: countSerializedChars(result.data),
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      const perPage = Math.min(
        typeof params.limit === 'number' ? params.limit : RAW_API_DEFAULT_LIMIT,
        GITHUB_SEARCH_MAX_LIMIT
      );
      return {
        data: {
          total_count: 0,
          items: [],
          nonExistentScope: true,
          pagination: {
            currentPage: params.page || 1,
            totalPages: 0,
            perPage,
            totalMatches: 0,
            reportedTotalMatches: 0,
            reachableTotalMatches: 0,
            totalMatchesKind: 'exact',
            totalMatchesCapped: false,
            hasMore: false,
          },
        },
        status: 200,
        rawResponseChars: 0,
      } as GitHubAPIResponse<OptimizedCodeSearchResult>;
    }
    const apiError = handleGitHubAPIError(error);
    return apiError;
  }
}

async function transformToOptimizedFormat(
  items: CodeSearchResultItem[],
  apiTotalCount?: number
): Promise<OptimizedCodeSearchResult> {
  const singleRepo = extractSingleRepository(items);

  const allMatchLocationsSet = new Set<string>();
  let hasMinificationFailures = false;
  const allMinificationTypes: string[] = [];

  const foundFiles = new Set<string>();

  const filteredItems = items.filter(item => !shouldIgnoreFile(item.path));

  let droppedItems = 0;
  let droppedMatches = 0;

  const itemResults = await Promise.allSettled(
    filteredItems.map(async item => {
      foundFiles.add(item.path);

      const itemMinificationTypes: string[] = [];

      const matchResults = await Promise.allSettled(
        (item.text_matches || []).map(async match => {
          let processedFragment = match.fragment;

          const sanitizationResult = ContentSanitizer.sanitizeContent(
            processedFragment || '',
            item.path
          );
          processedFragment = sanitizationResult.content;

          if (sanitizationResult.hasSecrets) {
            allMatchLocationsSet.add(
              `Secrets detected in ${item.path}: ${sanitizationResult.secretsDetected.join(', ')}`
            );
          }
          if (sanitizationResult.warnings.length > 0) {
            sanitizationResult.warnings.forEach((w: string) =>
              allMatchLocationsSet.add(`${item.path}: ${w}`)
            );
          }

          try {
            const minifyResult = await contextUtils.minifyContent(
              processedFragment || '',
              item.path
            );
            processedFragment = minifyResult.content;

            if (minifyResult.failed) {
              hasMinificationFailures = true;
            } else if (minifyResult.type !== 'failed') {
              itemMinificationTypes.push(minifyResult.type);
              allMinificationTypes.push(minifyResult.type);
            }
          } catch {
            hasMinificationFailures = true;
          }

          return {
            context: processedFragment || '',
            positions:
              match.matches?.map(m =>
                Array.isArray(m.indices) && m.indices.length >= 2
                  ? ([m.indices[0], m.indices[1]] as [number, number])
                  : ([0, 0] as [number, number])
              ) || [],
          };
        })
      );

      const processedMatches = matchResults
        .filter(
          (
            r
          ): r is PromiseFulfilledResult<{
            context: string;
            positions: [number, number][];
          }> => r.status === 'fulfilled'
        )
        .map(r => r.value);

      const rejectedMatchCount = matchResults.filter(
        r => r.status === 'rejected'
      ).length;
      if (rejectedMatchCount > 0) {
        droppedMatches += rejectedMatchCount;
      }

      const itemWithOptionalFields = item as CodeSearchResultItem & {
        last_modified_at?: string;
      };

      const uniqueItemTypes = Array.from(new Set(itemMinificationTypes));

      return {
        path: item.path,
        matches: processedMatches,
        url: item.html_url,
        repository: {
          nameWithOwner: item.repository.full_name,
          url: item.repository.url,
          pushedAt: item.repository.pushed_at || undefined,
        },
        ...(itemWithOptionalFields.last_modified_at && {
          lastModifiedAt: itemWithOptionalFields.last_modified_at,
        }),
        ...(uniqueItemTypes.length > 0 && {
          minificationType: uniqueItemTypes.join(','),
        }),
      };
    })
  );

  const optimizedItems = itemResults
    .filter(
      (
        r
      ): r is PromiseFulfilledResult<
        (typeof itemResults)[number] extends PromiseFulfilledResult<infer T>
          ? T
          : never
      > => r.status === 'fulfilled'
    )
    .map(r => r.value);

  droppedItems = itemResults.filter(r => r.status === 'rejected').length;

  const result: OptimizedCodeSearchResult = {
    items: optimizedItems,
    total_count:
      apiTotalCount !== undefined ? apiTotalCount : filteredItems.length,
    _researchContext: {
      uniqueFileCount: foundFiles.size,
      repositoryContext: singleRepo
        ? (() => {
            const parts = singleRepo.full_name.split('/');
            return parts.length === 2 && parts[0] && parts[1]
              ? {
                  owner: parts[0],
                  repo: parts[1],
                  branch: singleRepo.default_branch || undefined,
                }
              : undefined;
          })()
        : undefined,
    },
  };

  if (singleRepo) {
    result.repository = {
      name: singleRepo.full_name,
      url: singleRepo.url,
      createdAt: singleRepo.created_at || undefined,
      updatedAt: singleRepo.updated_at || undefined,
      pushedAt: singleRepo.pushed_at || undefined,
    };
  }

  if (droppedItems > 0) {
    allMatchLocationsSet.add(
      `${droppedItems} item(s) dropped due to processing errors`
    );
  }
  if (droppedMatches > 0) {
    allMatchLocationsSet.add(
      `${droppedMatches} match(es) dropped due to processing errors`
    );
  }

  if (allMatchLocationsSet.size > 0) {
    result.matchLocations = Array.from(allMatchLocationsSet);
  }

  result.minified = !hasMinificationFailures;
  result.minificationFailed = hasMinificationFailures;
  if (allMinificationTypes.length > 0) {
    result.minificationTypes = Array.from(new Set(allMinificationTypes));
  }

  return result;
}

function extractSingleRepository(items: CodeSearchResultItem[]) {
  if (items.length === 0) return null;

  const firstRepo = items[0]?.repository;
  if (!firstRepo) return null;
  const allSameRepo = items.every(
    item => item.repository.full_name === firstRepo.full_name
  );

  return allSameRepo ? firstRepo : null;
}
