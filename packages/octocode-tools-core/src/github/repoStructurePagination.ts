import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type { GitHubRepositoryStructureResult } from '../tools/github_view_repo_structure/types.js';
import { GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS } from '../tools/github_view_repo_structure/constants.js';
import { generateStructurePaginationHints } from '../utils/pagination/hints.js';

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
    params.itemsPerPage ?? STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE;
  const currentPage = params.page ?? 1;
  const totalEntries = cachedItems.length;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const startIdx = (currentPage - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);

  const paginatedItems = cachedItems.slice(startIdx, endIdx);

  const structure: Record<string, { files: string[]; folders: string[] }> =
    Object.create(null);
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
      return '.';
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
  > = Object.create(null);
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

  const hasMore = currentPage < totalPages;
  const paginationInfo = {
    currentPage,
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
    depth: params.maxDepth ?? 1,
    pageFiles,
    pageFolders,
    allFiles,
    allFolders,
  });

  let fileSizeMap: Record<string, Record<string, number>> | undefined;
  const cached = cachedResult._cachedFileSizeMap;
  if (cached) {
    const pageFilePaths = new Set(
      paginatedItems.filter(i => i.type === 'file').map(i => i.path)
    );
    const basePath2 = cachedResult.path === '/' ? '' : cachedResult.path;
    const map: Record<string, Record<string, number>> = Object.create(null);
    for (const [dirKey, dirFiles] of Object.entries(cached)) {
      for (const [fileName, size] of Object.entries(dirFiles)) {
        const fullPath =
          dirKey === '.'
            ? basePath2
              ? `${basePath2}/${fileName}`
              : fileName
            : basePath2
              ? `${basePath2}/${dirKey}/${fileName}`
              : `${dirKey}/${fileName}`;
        if (pageFilePaths.has(fullPath)) {
          if (!map[dirKey]) map[dirKey] = Object.create(null);
          map[dirKey]![fileName] = size;
        }
      }
    }
    if (Object.keys(map).length > 0) fileSizeMap = map;
  }

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
    ...(fileSizeMap !== undefined && { fileSizeMap }),
    ...(cached !== undefined && { _cachedFileSizeMap: cached }),
    pagination: paginationInfo,
    hints,
    rawResponseChars: cachedResult.rawResponseChars,
  };
}
