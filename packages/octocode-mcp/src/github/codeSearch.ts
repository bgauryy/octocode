import type {
  SearchCodeParameters,
  SearchCodeResponse,
  CodeSearchResultItem,
  GitHubAPIResponse,
  OptimizedCodeSearchResult,
} from './githubAPI';
import type { GitHubCodeSearchQuery } from '@octocodeai/octocode-core';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import { minifyContent } from '../utils/minifier/minifier.js';
import { getOctokit } from './client';
import { handleGitHubAPIError } from './errors';
import { buildCodeSearchQuery } from './queryBuilders';
import { generateCacheKey, withDataCache } from '../utils/http/cache';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { shouldIgnoreFile } from '../utils/file/filters';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';

export async function searchGitHubCodeAPI(
  params: GitHubCodeSearchQuery,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<OptimizedCodeSearchResult>> {
  // Cache key excludes context fields (mainResearchGoal, researchGoal, reasoning)
  // as they don't affect the API response
  const cacheKey = generateCacheKey(
    'gh-api-code',
    {
      keywordsToSearch: params.keywordsToSearch,
      owner: params.owner,
      repo: params.repo,
      extension: params.extension,
      filename: params.filename,
      path: params.path,
      match: params.match,
      limit: params.limit,
      page: params.page,
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
  params: GitHubCodeSearchQuery,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<OptimizedCodeSearchResult>> {
  try {
    const octokit = await getOctokit(authInfo);

    if (params.keywordsToSearch && params.keywordsToSearch.length > 0) {
      const validTerms = params.keywordsToSearch.filter(
        term => term && term.trim()
      );
      if (validTerms.length === 0) {
        await logSessionError(
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          SEARCH_ERRORS.QUERY_EMPTY.code
        );
        return {
          error: SEARCH_ERRORS.QUERY_EMPTY.message,
          type: 'http',
          status: 400,
        };
      }
    }

    const query = buildCodeSearchQuery(params);

    if (!query.trim()) {
      await logSessionError(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        SEARCH_ERRORS.QUERY_EMPTY.code
      );
      return {
        error: SEARCH_ERRORS.QUERY_EMPTY.message,
        type: 'http',
        status: 400,
      };
    }

    const perPage = Math.min(
      typeof params.limit === 'number' ? params.limit : 30,
      100
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

    const optimizedResult = await convertCodeSearchResult(result);

    // GitHub caps at 1000 total results
    const totalMatches = Math.min(optimizedResult.total_count, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;

    return {
      data: {
        total_count: optimizedResult.total_count,
        items: optimizedResult.items,
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
          hasMore,
        },
      },
      status: 200,
      headers: result.headers,
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);
    return apiError;
  }
}

async function convertCodeSearchResult(
  octokitResult: SearchCodeResponse
): Promise<OptimizedCodeSearchResult> {
  const items: CodeSearchResultItem[] = octokitResult.data.items;

  return transformToOptimizedFormat(items, octokitResult.data.total_count);
}

async function transformToOptimizedFormat(
  items: CodeSearchResultItem[],
  apiTotalCount?: number
): Promise<OptimizedCodeSearchResult> {
  const singleRepo = extractSingleRepository(items);

  const allMatchLocationsSet = new Set<string>();
  let hasMinificationFailures = false;
  const minificationTypes: string[] = [];

  const foundFiles = new Set<string>();

  const filteredItems = items.filter(item => !shouldIgnoreFile(item.path));

  let droppedItems = 0;
  let droppedMatches = 0;

  const itemResults = await Promise.allSettled(
    filteredItems.map(async item => {
      foundFiles.add(item.path);

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

          const minifyResult = await minifyContent(
            processedFragment || '',
            item.path
          );
          processedFragment = minifyResult.content;

          if (minifyResult.failed) {
            hasMinificationFailures = true;
          } else if (minifyResult.type !== 'failed') {
            minificationTypes.push(minifyResult.type);
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
        ...(minificationTypes.length > 0 && {
          minificationType: Array.from(new Set(minificationTypes)).join(','),
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
    // Use API total count if available, otherwise fallback to filtered length
    total_count:
      apiTotalCount !== undefined ? apiTotalCount : filteredItems.length,
    _researchContext: {
      foundFiles: Array.from(foundFiles),
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
  if (minificationTypes.length > 0) {
    result.minificationTypes = Array.from(new Set(minificationTypes));
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
