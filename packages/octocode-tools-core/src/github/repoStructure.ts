import { RequestError } from 'octokit';
import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type {
  GitHubApiFileItem,
  GitHubRepositoryStructureResult,
  GitHubRepositoryStructureError,
} from '../tools/github_view_repo_structure/types.js';
import { GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS } from '../tools/github_view_repo_structure/constants.js';
import { getOctokit, resolveDefaultBranch } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { generateStructurePaginationHints } from '../utils/pagination/hints.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { shouldIgnoreDir, shouldIgnoreFile } from '../utils/file/filters.js';
import { REPOSITORY_ERRORS } from '../errors/domainErrors.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';

import { applyStructurePagination } from './repoStructurePagination.js';
import {
  fetchDirectoryContentsRecursivelyAPI,
  getRecursiveFetchFailureCount,
} from './repoStructureRecursive.js';

import type { Octokit } from 'octokit';

type GitHubStructureFetchQuery = GitHubViewRepoStructureQuery & {
  includeSizes?: boolean;
};

interface ContentResolution {
  data: unknown;
  workingBranch: string;
  repoDefaultBranch?: string;
}

async function resolveContentWithBranchFallback(
  octokit: Octokit,
  owner: string,
  repo: string,
  cleanPath: string,
  branch: string | undefined,
  authInfo?: AuthInfo
): Promise<ContentResolution | GitHubRepositoryStructureError> {
  let workingBranch: string;
  // Capture the resolved default branch so callers get a `defaultBranch` hint.
  // Only known when we resolve it (no explicit branch given); when the caller
  // pinned a branch the repo default is unknown without an extra API call, so
  // the field stays absent rather than being fabricated.
  let repoDefaultBranch: string | undefined;
  try {
    if (branch) {
      workingBranch = branch;
    } else {
      repoDefaultBranch = await resolveDefaultBranch(owner, repo, authInfo);
      workingBranch = repoDefaultBranch;
    }
  } catch (repoError) {
    const apiError = handleGitHubAPIError(repoError);
    return {
      error: REPOSITORY_ERRORS.NOT_FOUND.message(owner, repo, apiError.error),
      status: apiError.status,
    };
  }

  try {
    const result = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: cleanPath || '',
      ref: workingBranch,
    });
    return {
      data: result.data,
      workingBranch,
      ...(repoDefaultBranch !== undefined ? { repoDefaultBranch } : {}),
    };
  } catch (error: unknown) {
    if (!(error instanceof RequestError && error.status === 404)) {
      const apiError = handleGitHubAPIError(error);
      return {
        error: REPOSITORY_ERRORS.ACCESS_FAILED.message(
          owner,
          repo,
          apiError.error
        ),
        status: apiError.status,
        rateLimitRemaining: apiError.rateLimitRemaining,
        rateLimitReset: apiError.rateLimitReset,
        retryAfter: apiError.retryAfter,
      };
    }

    const apiError = handleGitHubAPIError(error);
    return {
      error: REPOSITORY_ERRORS.PATH_NOT_FOUND.message(
        cleanPath,
        owner,
        repo,
        workingBranch
      ),
      status: apiError.status,
    };
  }
}

function mapApiItems(items: unknown[]): GitHubApiFileItem[] {
  return items.map(raw => {
    const item = raw as GitHubApiFileItem;
    return {
      name: item.name,
      path: item.path,
      type: item.type as 'file' | 'dir',
      size: 'size' in item ? item.size : undefined,
      download_url: 'download_url' in item ? item.download_url : undefined,
      url: item.url,
      html_url: item.html_url,
      git_url: item.git_url,
      sha: item.sha,
    } as GitHubApiFileItem;
  });
}

function buildStructureTree(
  items: GitHubApiFileItem[],
  basePath: string
): Record<string, { files: string[]; folders: string[] }> {
  const structure: Record<string, { files: string[]; folders: string[] }> =
    Object.create(null);

  const getRelativeParent = (itemPath: string): string => {
    let relativePath = itemPath;
    if (basePath && itemPath.startsWith(basePath)) {
      relativePath = itemPath.slice(basePath.length);
      if (relativePath.startsWith('/')) {
        relativePath = relativePath.slice(1);
      }
    }
    const lastSlash = relativePath.lastIndexOf('/');
    return lastSlash === -1 ? '.' : relativePath.slice(0, lastSlash);
  };

  const getItemName = (itemPath: string): string => {
    const lastSlash = itemPath.lastIndexOf('/');
    return lastSlash === -1 ? itemPath : itemPath.slice(lastSlash + 1);
  };

  for (const item of items) {
    const parentDir = getRelativeParent(item.path);
    if (!structure[parentDir]) {
      structure[parentDir] = { files: [], folders: [] };
    }
    const itemName = getItemName(item.path);
    if (item.type === 'file') {
      structure[parentDir].files.push(itemName);
    } else {
      structure[parentDir].folders.push(itemName);
    }
  }

  for (const entry of Object.values(structure)) {
    if (entry) {
      entry.files.sort();
      entry.folders.sort();
    }
  }

  const sortedKeys = Object.keys(structure).sort((a, b) => {
    if (a === '.') return -1;
    if (b === '.') return 1;
    return a.localeCompare(b);
  });
  const sortedStructure: Record<
    string,
    { files: string[]; folders: string[] }
  > = Object.create(null);
  for (const key of sortedKeys) {
    const entry = structure[key];
    if (entry) {
      sortedStructure[key] = entry;
    }
  }

  return sortedStructure;
}

function buildFileSizeMap(
  items: GitHubApiFileItem[],
  basePath: string
): Record<string, Record<string, number>> {
  const sizeMap: Record<string, Record<string, number>> = Object.create(null);
  for (const item of items) {
    if (item.type !== 'file' || item.size === undefined) continue;
    let relativePath = item.path;
    if (basePath && item.path.startsWith(basePath)) {
      relativePath = item.path.slice(basePath.length).replace(/^\//, '');
    }
    const lastSlash = relativePath.lastIndexOf('/');
    const dirKey = lastSlash === -1 ? '.' : relativePath.slice(0, lastSlash);
    const fileName =
      lastSlash === -1 ? relativePath : relativePath.slice(lastSlash + 1);
    if (!sizeMap[dirKey]) sizeMap[dirKey] = Object.create(null);
    sizeMap[dirKey]![fileName] = item.size;
  }
  return sizeMap;
}

export async function viewGitHubRepositoryStructureAPI(
  params: GitHubViewRepoStructureQuery,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubRepositoryStructureResult | GitHubRepositoryStructureError> {
  const cacheKey = generateCacheKey(
    'gh-repo-structure-api',
    {
      owner: params.owner,
      repo: params.repo,
      branch: params.branch,
      path: params.path,
      depth: params.maxDepth,
    },
    sessionId
  );

  const result = await withDataCache<
    GitHubRepositoryStructureResult | GitHubRepositoryStructureError
  >(
    cacheKey,
    async () => {
      return await viewGitHubRepositoryStructureAPIInternal(
        {
          ...params,
          itemsPerPage:
            params.itemsPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
          page: params.page ?? 1,
        },
        authInfo
      );
    },
    {
      shouldCache: value => !('error' in value),
    }
  );

  if (!('error' in result) && result.structure) {
    return applyStructurePagination(result, params);
  }

  return result;
}

async function viewGitHubRepositoryStructureAPIInternal(
  params: GitHubStructureFetchQuery,
  authInfo?: AuthInfo
): Promise<GitHubRepositoryStructureResult | GitHubRepositoryStructureError> {
  try {
    const octokit = await getOctokit(authInfo);
    const { owner, repo, branch, path = '', maxDepth: depth = 1 } = params;
    const cleanPath = path.replace(/^\/+|\/+$/g, '');

    const resolution = await resolveContentWithBranchFallback(
      octokit,
      owner,
      repo,
      cleanPath,
      branch,
      authInfo
    );
    if ('error' in resolution) return resolution;

    const { data, workingBranch, repoDefaultBranch } = resolution;
    let rawResponseChars = countSerializedChars(data);
    const rawItems = Array.isArray(data) ? data : [data];
    let allItems = mapApiItems(rawItems);
    let partialTreeFailures = 0;

    if (depth > 1) {
      const recursiveItems = await fetchDirectoryContentsRecursivelyAPI(
        octokit,
        owner,
        repo,
        workingBranch,
        cleanPath,
        1,
        depth
      );
      partialTreeFailures = getRecursiveFetchFailureCount(recursiveItems);
      rawResponseChars += getRawResponseChars(recursiveItems) ?? 0;
      const combined = [...allItems, ...recursiveItems];
      allItems = combined.filter(
        (item, index, array) =>
          array.findIndex(i => i.path === item.path) === index
      );
    }

    const filteredItems = allItems.filter(item =>
      item.type === 'dir'
        ? !shouldIgnoreDir(item.name)
        : !shouldIgnoreFile(item.path)
    );

    filteredItems.sort((a, b) => {
      if (a.type !== b.type) return a.type === 'dir' ? -1 : 1;
      const aDepth = a.path.split('/').length;
      const bDepth = b.path.split('/').length;
      if (aDepth !== bDepth) return aDepth - bDepth;
      return a.path.localeCompare(b.path);
    });

    const entriesPerPage =
      params.itemsPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE;
    const currentPage = params.page ?? 1;
    const totalEntries = filteredItems.length;
    const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
    const startIdx = (currentPage - 1) * entriesPerPage;
    const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
    const paginatedItems = filteredItems.slice(startIdx, endIdx);

    const sortedStructure = buildStructureTree(paginatedItems, cleanPath);

    const cachedFileSizeMap:
      Record<string, Record<string, number>> | undefined =
      params.includeSizes === true
        ? buildFileSizeMap(filteredItems, cleanPath)
        : undefined;
    const fileSizeMap: Record<string, Record<string, number>> | undefined =
      cachedFileSizeMap !== undefined
        ? buildFileSizeMap(paginatedItems, cleanPath)
        : undefined;

    const pageFiles = paginatedItems.filter(i => i.type === 'file').length;
    const pageFolders = paginatedItems.filter(i => i.type === 'dir').length;
    const allFiles = filteredItems.filter(i => i.type === 'file').length;
    const allFolders = filteredItems.filter(i => i.type === 'dir').length;
    const hasMore = currentPage < totalPages;

    const paginationInfo = {
      currentPage,
      totalPages,
      hasMore,
      ...(hasMore ? { nextPage: currentPage + 1 } : {}),
      entriesPerPage,
      totalEntries,
    };

    const hints = generateStructurePaginationHints(paginationInfo, {
      owner,
      repo,
      branch: workingBranch,
      path: cleanPath,
      depth,
      pageFiles,
      pageFolders,
      allFiles,
      allFolders,
    });

    if (partialTreeFailures > 0) {
      hints.unshift(
        `Partial tree: ${partialTreeFailures} subdirectory subtree(s) failed to load and are missing from this structure. The listing is incomplete — retry or narrow the path/depth.`
      );
    }

    return {
      owner,
      repo,
      branch: workingBranch,
      ...(repoDefaultBranch !== undefined && {
        defaultBranch: repoDefaultBranch,
      }),
      path: cleanPath || '/',
      apiSource: true,
      summary: {
        totalFiles: allFiles,
        totalFolders: allFolders,
        truncated: hasMore,
        filtered: true,
        originalCount: filteredItems.length,
      },
      structure: sortedStructure,
      ...(fileSizeMap !== undefined && { fileSizeMap }),
      ...(cachedFileSizeMap !== undefined && {
        _cachedFileSizeMap: cachedFileSizeMap,
      }),
      pagination: paginationInfo,
      hints,
      rawResponseChars,
      _cachedItems: filteredItems.map(item => ({
        path: item.path,
        type: item.type as 'file' | 'dir',
      })),
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);
    return {
      error: REPOSITORY_ERRORS.STRUCTURE_EXPLORATION_FAILED.message,
      status: apiError.status,
      rateLimitRemaining: apiError.rateLimitRemaining,
      rateLimitReset: apiError.rateLimitReset,
      retryAfter: apiError.retryAfter,
    };
  }
}
