/**
 * GitHub Repository Structure Operations
 * Orchestrates viewing and navigating repository directory structures.
 * Split into focused modules:
 *   - repoStructurePagination.ts: post-cache pagination application
 *   - repoStructureRecursive.ts: recursive directory content fetching
 */
import { RequestError } from 'octokit';
import type { GitHubViewRepoStructureQuery } from '@octocodeai/octocode-core';
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
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { REPOSITORY_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';

import { applyStructurePagination } from './repoStructurePagination.js';
import { fetchDirectoryContentsRecursivelyAPI } from './repoStructureRecursive.js';

import type { Octokit } from 'octokit';

const TOOL_NAME = TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE;

type GitHubStructureFetchQuery = Omit<
  GitHubViewRepoStructureQuery,
  'entriesPerPage' | 'entryPageNumber'
> &
  Partial<
    Pick<GitHubViewRepoStructureQuery, 'entriesPerPage' | 'entryPageNumber'>
  >;

interface ContentResolution {
  data: unknown;
  workingBranch: string;
  repoDefaultBranch?: string;
}

/**
 * Resolve repository content by trying the requested branch, the default
 * branch, and common fallback branches (main, master, develop).
 */
async function resolveContentWithBranchFallback(
  octokit: Octokit,
  owner: string,
  repo: string,
  cleanPath: string,
  branch: string | undefined,
  authInfo?: AuthInfo
): Promise<ContentResolution | GitHubRepositoryStructureError> {
  const workingBranch = branch ?? 'main';

  try {
    const result = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: cleanPath || undefined,
      ref: branch,
    });
    return { data: result.data, workingBranch };
  } catch (error: unknown) {
    if (!(error instanceof RequestError && error.status === 404)) {
      const apiError = handleGitHubAPIError(error);
      await logSessionError(TOOL_NAME, REPOSITORY_ERRORS.ACCESS_FAILED.code);
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

    let defaultBranch: string;
    let repoDefaultBranch: string | undefined;
    try {
      defaultBranch = await resolveDefaultBranch(owner, repo, authInfo);
      repoDefaultBranch = defaultBranch;
    } catch (repoError) {
      const apiError = handleGitHubAPIError(repoError);
      await logSessionError(TOOL_NAME, REPOSITORY_ERRORS.NOT_FOUND.code);
      return {
        error: REPOSITORY_ERRORS.NOT_FOUND.message(owner, repo, apiError.error),
        status: apiError.status,
      };
    }

    if (defaultBranch === branch) {
      const apiError = handleGitHubAPIError(error);
      await logSessionError(TOOL_NAME, REPOSITORY_ERRORS.PATH_NOT_FOUND.code);
      return {
        error: REPOSITORY_ERRORS.PATH_NOT_FOUND.message(
          cleanPath,
          owner,
          repo,
          branch
        ),
        status: apiError.status,
      };
    }

    const branchCandidates = [
      defaultBranch,
      ...['main', 'master', 'develop'].filter(
        b => b !== branch && b !== defaultBranch
      ),
    ];

    for (const candidate of branchCandidates) {
      try {
        const result = await octokit.rest.repos.getContent({
          owner,
          repo,
          path: cleanPath || undefined,
          ref: candidate,
        });
        return {
          data: result.data,
          workingBranch: candidate,
          repoDefaultBranch,
        };
      } catch {
        // Path/ref missing on this branch; try the next candidate
      }
    }

    const apiError = handleGitHubAPIError(error);
    await logSessionError(
      TOOL_NAME,
      REPOSITORY_ERRORS.PATH_NOT_FOUND_ANY_BRANCH.code
    );
    return {
      error: REPOSITORY_ERRORS.PATH_NOT_FOUND_ANY_BRANCH.message(
        cleanPath,
        owner,
        repo
      ),
      status: apiError.status,
      triedBranches: [branch, ...branchCandidates].filter(
        (b): b is string => b !== undefined
      ),
      defaultBranch,
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
  const structure: Record<string, { files: string[]; folders: string[] }> = {};

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
  > = {};
  for (const key of sortedKeys) {
    const entry = structure[key];
    if (entry) {
      sortedStructure[key] = entry;
    }
  }

  return sortedStructure;
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
      depth: params.depth,
    },
    sessionId
  );

  const result = await withDataCache<
    GitHubRepositoryStructureResult | GitHubRepositoryStructureError
  >(
    cacheKey,
    async () => {
      // The cached fetch returns the full subtree; per-page slicing
      // happens at the call site (applyStructurePagination). The inner
      // API type requires entriesPerPage/entryPageNumber, but they have
      // no effect on the network call — pass placeholder values and let
      // the post-processing apply the real pagination.
      return await viewGitHubRepositoryStructureAPIInternal(
        {
          ...params,
          entriesPerPage:
            params.entriesPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
          entryPageNumber: params.entryPageNumber ?? 1,
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
    const { owner, repo, branch, path = '', depth = 1 } = params;
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
      params.entriesPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE;
    const entryPageNumber = params.entryPageNumber ?? 1;
    const totalEntries = filteredItems.length;
    const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
    const startIdx = (entryPageNumber - 1) * entriesPerPage;
    const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
    const paginatedItems = filteredItems.slice(startIdx, endIdx);

    const sortedStructure = buildStructureTree(paginatedItems, cleanPath);

    const pageFiles = paginatedItems.filter(i => i.type === 'file').length;
    const pageFolders = paginatedItems.filter(i => i.type === 'dir').length;
    const allFiles = filteredItems.filter(i => i.type === 'file').length;
    const allFolders = filteredItems.filter(i => i.type === 'dir').length;
    const hasMore = entryPageNumber < totalPages;

    const paginationInfo = {
      currentPage: entryPageNumber,
      totalPages,
      hasMore,
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

    const noPaginationRequested =
      params.entriesPerPage === undefined &&
      params.entryPageNumber === undefined;

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
      pagination: paginationInfo,
      hints,
      rawResponseChars,
      ...(noPaginationRequested && {
        _cachedItems: filteredItems.map(item => ({
          path: item.path,
          type: item.type as 'file' | 'dir',
        })),
      }),
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);
    await logSessionError(
      TOOL_NAME,
      REPOSITORY_ERRORS.STRUCTURE_EXPLORATION_FAILED.code
    );
    return {
      error: REPOSITORY_ERRORS.STRUCTURE_EXPLORATION_FAILED.message,
      status: apiError.status,
      rateLimitRemaining: apiError.rateLimitRemaining,
      rateLimitReset: apiError.rateLimitReset,
      retryAfter: apiError.retryAfter,
    };
  }
}
