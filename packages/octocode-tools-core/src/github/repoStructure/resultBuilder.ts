import type {
  GitHubApiFileItem,
  GitHubRepositoryStructureResult,
} from '../../tools/github_view_repo_structure/types.js';
import { GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS } from '../../tools/github_view_repo_structure/constants.js';
import { generateStructurePaginationHints } from '../../utils/pagination/hints.js';
import { shouldIgnoreDir, shouldIgnoreFile } from '../../utils/file/filters.js';

export function buildStructureTree(
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

export function buildFileSizeMap(
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

export function buildStructureResult(args: {
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
