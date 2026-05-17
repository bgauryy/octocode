/**
 * GitLab Repository Structure
 *
 * Get repository tree/structure from GitLab.
 * Note: File size is NOT included in tree response.
 *
 * @module gitlab/repoStructure
 */

import type {
  GitLabAPIResponse,
  GitLabTreeQuery,
  GitLabTreeItem,
} from './types.js';
import { getGitlab } from './client.js';
import { handleGitLabAPIError, createGitLabError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { shouldIgnoreDir, shouldIgnoreFile } from '../utils/file/filters.js';
import {
  isGitLabProject,
  isGitLabTreeItem,
  parseGitLabArray,
} from './responseGuards.js';

/**
 * Repository structure result.
 */
interface GitLabRepoStructureResult {
  projectId: number | string;
  projectPath: string;
  branch: string;
  path: string;
  summary: {
    totalFiles: number;
    totalFolders: number;
    truncated: boolean;
    filtered: boolean;
    originalCount: number;
  };
  structure: Record<string, { files: string[]; folders: string[] }>;
  pagination?: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    entriesPerPage: number;
    totalEntries: number;
  };
  hints?: string[];
  // Internal: cached items for pagination
  _cachedItems?: Array<{ path: string; type: 'file' | 'dir' }>;
}

/**
 * Get GitLab repository structure.
 *
 * @param params - Query parameters
 * @param sessionId - Optional session ID for caching
 * @returns Repository structure
 */
export async function viewGitLabRepositoryStructureAPI(
  params: GitLabTreeQuery,
  sessionId?: string
): Promise<GitLabAPIResponse<GitLabRepoStructureResult>> {
  // Validate required parameters
  if (!params.projectId) {
    return createGitLabError('Project ID is required', 400);
  }

  // Generate cache key for full tree (without pagination params)
  const fullCacheKey = generateCacheKey(
    'gl-api-tree-full',
    {
      projectId: params.projectId,
      ref: params.ref,
      path: params.path,
      recursive: params.recursive,
    },
    sessionId
  );

  const fullResult = await withDataCache<
    GitLabAPIResponse<GitLabRepoStructureResult>
  >(fullCacheKey, async () => fetchGitLabTreeFull(params), {
    shouldCache: value => 'data' in value && !('error' in value),
  });

  if ('error' in fullResult || !fullResult.data) {
    return fullResult;
  }

  return applyStructurePagination(fullResult.data, params);
}

async function fetchGitLabTreeFull(
  params: GitLabTreeQuery
): Promise<GitLabAPIResponse<GitLabRepoStructureResult>> {
  try {
    const gitlab = await getGitlab();

    const project = await gitlab.Projects.show(params.projectId);
    if (!isGitLabProject(project)) {
      return createGitLabError('Unexpected GitLab project response shape', 502);
    }
    const workingRef = params.ref || project.default_branch || 'main';

    const treeOptions: Record<string, unknown> = {
      ref: workingRef,
      path: params.path || undefined,
      recursive: params.recursive !== false, // Default to recursive
      perPage: 100,
    };

    Object.keys(treeOptions).forEach(key => {
      if (treeOptions[key] === undefined) {
        delete treeOptions[key];
      }
    });

    const allItems = parseGitLabArray(
      await gitlab.Repositories.allRepositoryTrees(
        params.projectId,
        treeOptions
      ),
      isGitLabTreeItem
    );
    if (!allItems) {
      return createGitLabError(
        'Unexpected GitLab repository tree response shape',
        502
      );
    }

    const filteredItems = allItems.filter(item => {
      if (item.type === 'tree') {
        return !shouldIgnoreDir(item.name);
      }
      return !shouldIgnoreFile(item.path);
    });

    filteredItems.sort((a, b) => {
      if (a.type !== b.type) {
        return a.type === 'tree' ? -1 : 1;
      }
      return a.path.localeCompare(b.path);
    });

    const cachedItems = filteredItems.map(item => ({
      path: item.path,
      type: item.type === 'tree' ? ('dir' as const) : ('file' as const),
    }));

    const totalFiles = filteredItems.filter(i => i.type === 'blob').length;
    const totalFolders = filteredItems.filter(i => i.type === 'tree').length;

    return {
      data: {
        projectId: params.projectId,
        projectPath: project.path_with_namespace,
        branch: workingRef,
        path: params.path || '/',
        summary: {
          totalFiles,
          totalFolders,
          truncated: false,
          filtered: true,
          originalCount: filteredItems.length,
        },
        structure: {}, // Will be populated by applyStructurePagination
        _cachedItems: cachedItems,
        hints: [`Project: ${project.path_with_namespace}`],
      },
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

function applyStructurePagination(
  result: GitLabRepoStructureResult,
  params: GitLabTreeQuery
): GitLabAPIResponse<GitLabRepoStructureResult> {
  const cachedItems = result._cachedItems;

  if (!cachedItems || cachedItems.length === 0) {
    const { _cachedItems, ...cleanResult } = result;
    return { data: cleanResult, status: 200 };
  }

  const entriesPerPage = params.perPage || 20;
  const entryPageNumber = params.page || 1;
  const totalEntries = cachedItems.length;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const startIdx = (entryPageNumber - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
  const paginatedItems = cachedItems.slice(startIdx, endIdx);

  const structure = buildStructureFromItems(paginatedItems, result.path);
  const hasMore = entryPageNumber < totalPages;

  const { _cachedItems, ...cleanResult } = result;

  return {
    data: {
      ...cleanResult,
      structure,
      summary: {
        ...cleanResult.summary,
        truncated: hasMore,
      },
      pagination: {
        currentPage: entryPageNumber,
        totalPages,
        hasMore,
        entriesPerPage,
        totalEntries,
      },
      hints: [
        ...(hasMore
          ? [
              `Page ${entryPageNumber}/${totalPages}. Use page=${entryPageNumber + 1} for more.`,
            ]
          : []),
        `Project: ${cleanResult.projectPath}`,
      ],
    },
    status: 200,
  };
}

function buildStructureFromItems(
  items: Array<{ path: string; type: 'file' | 'dir' }>,
  basePath: string
): Record<string, { files: string[]; folders: string[] }> {
  const structure: Record<string, { files: string[]; folders: string[] }> = {};
  const normalizedBase = basePath === '/' ? '' : basePath;

  const getRelativeParent = (itemPath: string): string => {
    let relativePath = itemPath;
    if (normalizedBase && itemPath.startsWith(normalizedBase)) {
      relativePath = itemPath.slice(normalizedBase.length);
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

  // Sort within each directory
  for (const dir of Object.keys(structure)) {
    const entry = structure[dir];
    if (entry) {
      entry.files.sort();
      entry.folders.sort();
    }
  }

  return structure;
}

/**
 * Transform GitLab tree to unified format.
 */
export function transformGitLabTree(
  items: GitLabTreeItem[]
): Array<{ path: string; type: 'file' | 'dir'; name: string }> {
  return items.map(item => ({
    path: item.path,
    type: item.type === 'tree' ? 'dir' : 'file',
    name: item.name,
  }));
}
