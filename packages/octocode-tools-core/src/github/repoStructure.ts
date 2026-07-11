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
import { getOctokit, resolveDefaultBranch, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { generateCacheKey, withDataCacheConditional } from '../utils/http/cache.js';
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
import {
  fetchStructureViaGitTree,
  isGitStructureTreesEnabled,
} from './repoStructureTree.js';
import { extractEtag } from './responseHeaders.js';

import type { Octokit } from 'octokit';

type GitHubStructureFetchQuery = GitHubViewRepoStructureQuery & {
  includeSizes?: boolean;
};

type StructureFetchOutcome = {
  result: GitHubRepositoryStructureResult | GitHubRepositoryStructureError;
  etag?: string;
  notModified?: boolean;
};

interface ContentResolution {
  data: unknown;
  workingBranch: string;
  repoDefaultBranch?: string;
  etag?: string;
  notModified?: boolean;
}

async function resolveContentWithBranchFallback(
  octokit: Octokit,
  owner: string,
  repo: string,
  cleanPath: string,
  branch: string | undefined,
  authInfo?: AuthInfo,
  ifNoneMatch?: string
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
      ...(ifNoneMatch
        ? { headers: { 'If-None-Match': ifNoneMatch } }
        : {}),
    });
    const etag = extractEtag(result.headers);
    return {
      data: result.data,
      workingBranch,
      ...(repoDefaultBranch !== undefined ? { repoDefaultBranch } : {}),
      ...(etag ? { etag } : {}),
    };
  } catch (error: unknown) {
    if (error instanceof RequestError && error.status === 304) {
      return {
        data: null,
        workingBranch,
        ...(repoDefaultBranch !== undefined ? { repoDefaultBranch } : {}),
        etag: ifNoneMatch,
        notModified: true,
      };
    }
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
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-repo-structure-api',
    {
      owner: params.owner,
      repo: params.repo,
      branch: params.branch,
      path: params.path,
      depth: params.maxDepth,
      auth,
    },
    sessionId
  );

  const result = await withDataCacheConditional<
    GitHubRepositoryStructureResult | GitHubRepositoryStructureError
  >(
    cacheKey,
    async ({ ifNoneMatch }) => {
      const outcome = await viewGitHubRepositoryStructureAPIInternal(
        {
          ...params,
          itemsPerPage:
            params.itemsPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
          page: params.page ?? 1,
        },
        authInfo,
        ifNoneMatch
      );
      return {
        value: outcome.result,
        etag: outcome.etag,
        notModified: outcome.notModified,
      };
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
  authInfo?: AuthInfo,
  ifNoneMatch?: string
): Promise<StructureFetchOutcome> {
  try {
    const octokit = await getOctokit(authInfo);
    const { owner, repo, branch, path = '', maxDepth: depth = 1 } = params;
    const cleanPath = path.replace(/^\/+|\/+$/g, '');

    // Depth 1: single Contents listing. Depth > 1: prefer recursive Git Trees
    // (O(1) API calls) unless OCTOCODE_GH_STRUCTURE_TREES=0.
    if (depth > 1 && isGitStructureTreesEnabled()) {
      return await viewStructureViaTrees(
        octokit,
        params,
        cleanPath,
        depth,
        authInfo,
        ifNoneMatch
      );
    }

    const resolution = await resolveContentWithBranchFallback(
      octokit,
      owner,
      repo,
      cleanPath,
      branch,
      authInfo,
      // Conditional GET only for single Contents listing (depth 1). Recursive
      // Contents walks many paths — one ETag cannot cover the whole result.
      depth === 1 ? ifNoneMatch : undefined
    );
    if ('error' in resolution) return { result: resolution };
    if (resolution.notModified) {
      return {
        result: {
          error: 'not-modified',
          status: 304,
        },
        etag: resolution.etag ?? ifNoneMatch,
        notModified: true,
      };
    }

    const { data, workingBranch, repoDefaultBranch, etag } = resolution;
    let rawResponseChars = countSerializedChars(data);
    const rawItems = Array.isArray(data) ? data : [data];
    let allItems = mapApiItems(rawItems);
    let partialTreeFailures = 0;

    if (depth > 1) {
      // Contents fallback: recursive fetch already loads the root path — do not
      // keep the duplicate root listing from resolveContentWithBranchFallback.
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
      rawResponseChars = getRawResponseChars(recursiveItems) ?? 0;
      allItems = recursiveItems;
    }

    return {
      result: buildStructureResult({
        owner,
        repo,
        workingBranch,
        repoDefaultBranch,
        cleanPath,
        depth,
        allItems,
        partialTreeFailures,
        incompleteTree: false,
        rawResponseChars,
        includeSizes: params.includeSizes === true,
        itemsPerPage: params.itemsPerPage,
        page: params.page,
      }),
      // Soft ETag only for single-call depth-1 Contents (stable body ↔ etag).
      ...(depth === 1 && etag ? { etag } : {}),
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);
    return {
      result: {
        error: REPOSITORY_ERRORS.STRUCTURE_EXPLORATION_FAILED.message,
        status: apiError.status,
        rateLimitRemaining: apiError.rateLimitRemaining,
        rateLimitReset: apiError.rateLimitReset,
        retryAfter: apiError.retryAfter,
      },
    };
  }
}

async function viewStructureViaTrees(
  octokit: Octokit,
  params: GitHubStructureFetchQuery,
  cleanPath: string,
  depth: number,
  authInfo?: AuthInfo,
  ifNoneMatch?: string
): Promise<StructureFetchOutcome> {
  const { owner, repo, branch } = params;
  let workingBranch: string;
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
      result: {
        error: REPOSITORY_ERRORS.NOT_FOUND.message(owner, repo, apiError.error),
        status: apiError.status,
      },
    };
  }

  let treeResult;
  try {
    treeResult = await fetchStructureViaGitTree(octokit, {
      owner,
      repo,
      workingBranch,
      pathPrefix: cleanPath,
      maxDepth: depth,
      ifNoneMatch,
    });
  } catch (error: unknown) {
    // Trees failed (missing ref, etc.) — fall back to Contents recursion.
    const resolution = await resolveContentWithBranchFallback(
      octokit,
      owner,
      repo,
      cleanPath,
      workingBranch,
      authInfo
    );
    if ('error' in resolution) return { result: resolution };
    const recursiveItems = await fetchDirectoryContentsRecursivelyAPI(
      octokit,
      owner,
      repo,
      resolution.workingBranch,
      cleanPath,
      1,
      depth
    );
    return {
      result: buildStructureResult({
        owner,
        repo,
        workingBranch: resolution.workingBranch,
        repoDefaultBranch,
        cleanPath,
        depth,
        allItems: recursiveItems,
        partialTreeFailures: getRecursiveFetchFailureCount(recursiveItems),
        incompleteTree: false,
        rawResponseChars: getRawResponseChars(recursiveItems) ?? 0,
        includeSizes: params.includeSizes === true,
        itemsPerPage: params.itemsPerPage,
        page: params.page,
        extraHints: [
          `Git Trees fetch failed (${error instanceof Error ? error.message : String(error)}); used Contents recursion instead.`,
        ],
      }),
    };
  }

  if (treeResult.notModified) {
    return {
      result: { error: 'not-modified', status: 304 },
      etag: treeResult.etag ?? ifNoneMatch,
      notModified: true,
    };
  }

  let allItems = treeResult.items;
  let partialTreeFailures = 0;
  let rawResponseChars = treeResult.rawResponseChars;
  const incompleteTree = treeResult.truncated;
  const extraHints: string[] = [];

  if (incompleteTree) {
    extraHints.push(
      'Git Trees response was truncated by GitHub — this structure listing may be incomplete. Narrow path/depth or set OCTOCODE_GH_STRUCTURE_TREES=0 for Contents recursion.'
    );
    try {
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
    } catch {
      void 0;
    }
  }

  return {
    result: buildStructureResult({
      owner,
      repo,
      workingBranch,
      repoDefaultBranch,
      cleanPath,
      depth,
      allItems,
      partialTreeFailures,
      incompleteTree,
      rawResponseChars,
      includeSizes: params.includeSizes === true,
      itemsPerPage: params.itemsPerPage,
      page: params.page,
      extraHints,
    }),
    ...(treeResult.etag && !incompleteTree ? { etag: treeResult.etag } : {}),
  };
}

function buildStructureResult(args: {
  owner: string;
  repo: string;
  workingBranch: string;
  repoDefaultBranch?: string;
  cleanPath: string;
  depth: number;
  allItems: GitHubApiFileItem[];
  partialTreeFailures: number;
  incompleteTree: boolean;
  rawResponseChars: number;
  includeSizes: boolean;
  itemsPerPage?: number;
  page?: number;
  extraHints?: string[];
}): GitHubRepositoryStructureResult {
  const {
    owner,
    repo,
    workingBranch,
    repoDefaultBranch,
    cleanPath,
    depth,
    partialTreeFailures,
    incompleteTree,
    rawResponseChars,
    includeSizes,
    extraHints = [],
  } = args;

  const filteredItems = args.allItems.filter(item =>
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
    args.itemsPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE;
  const currentPage = args.page ?? 1;
  const totalEntries = filteredItems.length;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const startIdx = (currentPage - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
  const paginatedItems = filteredItems.slice(startIdx, endIdx);

  const sortedStructure = buildStructureTree(paginatedItems, cleanPath);

  const cachedFileSizeMap: Record<string, Record<string, number>> | undefined =
    includeSizes ? buildFileSizeMap(filteredItems, cleanPath) : undefined;
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
  for (const hint of extraHints) {
    hints.unshift(hint);
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
      ...(incompleteTree ? { incompleteTree: true } : {}),
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
}
