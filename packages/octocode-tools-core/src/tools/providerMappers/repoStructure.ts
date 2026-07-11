import type { RepoStructureResult as ProviderRepoStructureResult } from '../../providers/types.js';
import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { WithOptionalMeta } from '../../types/execution.js';

import { GITHUB_STRUCTURE_DEFAULTS } from '../github_view_repo_structure/constants.js';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
type PartialRepoStructureQuery = WithOptionalMeta<GitHubViewRepoStructureQuery>;

export function mapRepoStructureToolQuery(
  query: PartialRepoStructureQuery,
  resolvedBranch: string
) {
  return {
    projectId: `${query.owner}/${query.repo}`,
    ref: resolvedBranch,
    path: query.path ? String(query.path) : undefined,
    depth: typeof query.maxDepth === 'number' ? query.maxDepth : undefined,
    itemsPerPage:
      (query as { itemsPerPage?: number }).itemsPerPage ??
      GITHUB_STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
    page: (() => {
      const page = (query as { page?: number }).page;
      return typeof page === 'number' ? page : undefined;
    })(),
    includeSizes: (query as { includeSizes?: boolean }).includeSizes,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapRepoStructureProviderResult(
  data: ProviderRepoStructureResult,
  _query: PartialRepoStructureQuery,
  filteredStructure: ProviderRepoStructureResult['structure'],
  resolvedBranch: string
): Record<string, unknown> {
  const requestedBranch = resolvedBranch;
  const actualBranch = data.branch ?? resolvedBranch;
  const branchFellBack =
    requestedBranch &&
    actualBranch &&
    requestedBranch !== actualBranch &&
    requestedBranch !== 'HEAD';

  const structureArray = Object.entries(filteredStructure)
    .sort(([a], [b]) => (a === '.' ? -1 : b === '.' ? 1 : a.localeCompare(b)))
    .map(([dir, entry]) => ({
      dir,
      files: entry.files,
      folders: entry.folders,
    }));

  const fileSizeMap = (
    data as { fileSizeMap?: Record<string, Record<string, number>> }
  ).fileSizeMap;
  const fileSizes: Record<string, number> = {};
  if (fileSizeMap) {
    for (const [dirPath, dirFiles] of Object.entries(fileSizeMap)) {
      if (filteredStructure[dirPath]) {
        const allowedFiles = new Set(filteredStructure[dirPath]!.files);
        for (const [fileName, size] of Object.entries(dirFiles)) {
          if (allowedFiles.has(fileName)) {
            // Key by full relative path so identically named files in
            // different directories don't collide onto one bare-name entry.
            const relativePath =
              dirPath === '.' ? fileName : `${dirPath}/${fileName}`;
            fileSizes[relativePath] = size;
          }
        }
      }
    }
  }

  // Filtering happens after provider pagination, so the provider's summary
  // counts ignored files/folders that were stripped. Recompute from the
  // filtered structure so the summary describes what is actually emitted.
  const filteredSummary = Object.values(filteredStructure).reduce(
    (totals, entry) => {
      totals.totalFiles += entry.files.length;
      totals.totalFolders += entry.folders.length;
      return totals;
    },
    { totalFiles: 0, totalFolders: 0 }
  );

  const resultData: Record<string, unknown> = {
    structure: structureArray,
    ...(Object.keys(fileSizes).length > 0 && { fileSizes }),
    summary: {
      totalFiles: filteredSummary.totalFiles,
      totalFolders: filteredSummary.totalFolders,
    },
  };

  if (actualBranch) {
    resultData.resolvedBranch = actualBranch;
  }

  if (branchFellBack) {
    resultData.branchFallback = {
      requestedBranch,
      actualBranch,
      ...(data.defaultBranch !== undefined && {
        defaultBranch: data.defaultBranch,
      }),
      warning: `Branch '${requestedBranch}' not found. Showing '${actualBranch}' (default branch). Re-query with the correct branch name if branch-specific results are required.`,
    };
  }

  if (
    data.pagination &&
    (data.pagination.hasMore || data.pagination.totalPages > 1)
  ) {
    resultData.pagination = data.pagination;
  }

  return resultData;
}
