import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  RepoStructureQuery,
  RepoStructureResult,
} from '../types.js';

import { viewGitHubRepositoryStructureAPI } from '../../github/repoStructure.js';

import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type { GitHubRepositoryStructureResult } from '../../tools/github_view_repo_structure/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import {
  createGitHubProviderErrorFromResult,
  parseGitHubProjectId,
} from './utils.js';
export { parseGitHubProjectId } from './utils.js';

export function transformRepoStructureResult(
  data: GitHubRepositoryStructureResult
): RepoStructureResult {
  return {
    projectPath: `${data.owner}/${data.repo}`,
    branch: data.branch || '',
    ...(data.defaultBranch !== undefined && {
      defaultBranch: data.defaultBranch,
    }),
    path: data.path || '/',
    structure: data.structure || {},
    ...(data.fileSizeMap !== undefined && { fileSizeMap: data.fileSizeMap }),
    // _cachedFileSizeMap is an internal field — never leak it to consumers
    summary: {
      totalFiles: data.summary?.totalFiles || 0,
      totalFolders: data.summary?.totalFolders || 0,
      truncated: data.summary?.truncated || false,
    },
    pagination: data.pagination,
    hints: data.hints,
  };
}

export async function getRepoStructure(
  query: RepoStructureQuery,
  authInfo?: AuthInfo,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): Promise<ProviderResponse<RepoStructureResult>> {
  const { owner, repo } = parseProjectId(query.projectId);

  if (!owner || !repo) {
    return {
      error: 'Project ID is required for repository structure',
      status: 400,
      provider: 'github',
    };
  }

  const githubQuery = {
    owner,
    repo,
    branch: query.ref || 'HEAD',
    path: query.path,
    maxDepth: query.depth,
    itemsPerPage: query.itemsPerPage,
    page: query.page,
    includeSizes: query.includeSizes,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GitHubViewRepoStructureQuery & { includeSizes?: boolean };

  const result = await viewGitHubRepositoryStructureAPI(githubQuery, authInfo);

  if ('error' in result) {
    return (
      createGitHubProviderErrorFromResult(result) ?? {
        error: 'Unknown GitHub API error',
        status: 500,
        provider: 'github',
      }
    );
  }

  return {
    data: transformRepoStructureResult(result),
    status: 200,
    provider: 'github',
    rawResponseChars: result.rawResponseChars ?? countSerializedChars(result),
  };
}
