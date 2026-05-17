/**
 * Repository structure pagination — applies pagination to cached structure results.
 * Extracted from repoStructure.ts.
 */
import type { GitHubViewRepoStructureQuery } from '@octocodeai/octocode-core';
import type { GitHubRepositoryStructureResult } from '../tools/github_view_repo_structure/types.js';
import { GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS } from '../tools/github_view_repo_structure/constants.js';
import { generateStructurePaginationHints } from '../utils/pagination/hints.js';

/**
 * Apply pagination to cached structure result
 * Rebuilds structure from cached items based on pagination params
 */
export function applyStructurePagination(
  cachedResult: GitHubRepositoryStructureResult,
  params: GitHubViewRepoStructureQuery
): GitHubRepositoryStructureResult {
  const cachedItems = cachedResult._cachedItems;

  if (!cachedItems || cachedItems.length === 0) {
    const { _cachedItems, ...result } = cachedResult;
    return result;
  }

  const entriesPerPage =
    params.entriesPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE;
  const entryPageNumber = params.entryPageNumber ?? 1;
  const totalEntries = cachedItems.length;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const startIdx = (entryPageNumber - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);

  const paginatedItems = cachedItems.slice(startIdx, endIdx);

  const structure: Record<string, { files: string[]; folders: string[] }> = {};
  const basePath = cachedResult.path === '/' ? '' : cachedResult.path;

  const getRelativeParent = (itemPath: string): string => {
    let relativePath = itemPath;
    if (basePath && itemPath.startsWith(basePath)) {
      relativePath = itemPath.slice(basePath.length);
      if (relativePath.startsWith('/')) {
        relativePath = relativePath.slice(1);
      }
    }

    const lastSlash = relativePath.lastIndexOf('/');
    if (lastSlash === -1) {
      return '.'; // Root level
    }
    return relativePath.slice(0, lastSlash);
  };

  const getItemName = (itemPath: string): string => {
    const lastSlash = itemPath.lastIndexOf('/');
    return lastSlash === -1 ? itemPath : itemPath.slice(lastSlash + 1);
  };

  for (const item of paginatedItems) {
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

  for (const dir of Object.keys(structure)) {
    const entry = structure[dir];
    if (entry) {
      entry.files.sort();
      entry.folders.sort();
    }
  }

  const sortedStructure: Record<
    string,
    { files: string[]; folders: string[] }
  > = {};
  const sortedKeys = Object.keys(structure).sort((a, b) => {
    if (a === '.') return -1;
    if (b === '.') return 1;
    return a.localeCompare(b);
  });
  for (const key of sortedKeys) {
    const entry = structure[key];
    if (entry) {
      sortedStructure[key] = entry;
    }
  }

  const pageFiles = paginatedItems.filter(i => i.type === 'file').length;
  const pageFolders = paginatedItems.filter(i => i.type === 'dir').length;
  const allFiles = cachedItems.filter(i => i.type === 'file').length;
  const allFolders = cachedItems.filter(i => i.type === 'dir').length;

  const hasMore = entryPageNumber < totalPages;
  const paginationInfo = {
    currentPage: entryPageNumber,
    totalPages,
    hasMore,
    entriesPerPage,
    totalEntries,
  };

  const hints = generateStructurePaginationHints(paginationInfo, {
    owner: cachedResult.owner,
    repo: cachedResult.repo,
    branch: cachedResult.branch,
    path: basePath,
    depth: params.depth ?? 1,
    pageFiles,
    pageFolders,
    allFiles,
    allFolders,
  });

  return {
    owner: cachedResult.owner,
    repo: cachedResult.repo,
    branch: cachedResult.branch,
    path: cachedResult.path,
    apiSource: cachedResult.apiSource,
    summary: {
      totalFiles: allFiles,
      totalFolders: allFolders,
      truncated: hasMore,
      filtered: true,
      originalCount: totalEntries,
    },
    structure: sortedStructure,
    pagination: paginationInfo,
    hints,
    rawResponseChars: cachedResult.rawResponseChars,
  };
}
