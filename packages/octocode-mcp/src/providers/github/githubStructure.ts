/**
 * GitHub Repository Structure
 *
 * Extracted from GitHubProvider for better modularity.
 *
 * @module providers/github/githubStructure
 */

import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  RepoStructureQuery,
  RepoStructureResult,
} from '../types.js';

import { viewGitHubRepositoryStructureAPI } from '../../github/repoStructure.js';

import type { GitHubViewRepoStructureQuery } from '@octocodeai/octocode-core';
import type { GitHubRepositoryStructureResult } from '../../tools/github_view_repo_structure/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';
export { parseGitHubProjectId } from './utils.js';

/**
 * Transform GitHub repository structure result to unified format.
 */
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
    summary: {
      totalFiles: data.summary?.totalFiles || 0,
      totalFolders: data.summary?.totalFolders || 0,
      truncated: data.summary?.truncated || false,
    },
    pagination: data.pagination,
    hints: data.hints,
  };
}

/**
 * Get repository structure from GitHub.
 */
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
    depth: query.depth,
    entriesPerPage: query.entriesPerPage,
    entryPageNumber: query.entryPageNumber,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GitHubViewRepoStructureQuery;

  const result = await viewGitHubRepositoryStructureAPI(githubQuery, authInfo);

  if ('error' in result) {
    const errorResult = result as {
      error: string | { toString(): string };
      status?: number;
      hints?: string[];
      rateLimitRemaining?: number;
      rateLimitReset?: number;
      retryAfter?: number;
    };
    return createGitHubProviderError({
      error:
        typeof errorResult.error === 'string'
          ? errorResult.error
          : String(errorResult.error),
      status: errorResult.status || 500,
      hints: errorResult.hints,
      rateLimitRemaining: errorResult.rateLimitRemaining,
      rateLimitReset: errorResult.rateLimitReset,
      retryAfter: errorResult.retryAfter,
    });
  }

  return {
    data: transformRepoStructureResult(result),
    status: 200,
    provider: 'github',
    rawResponseChars: result.rawResponseChars ?? countSerializedChars(result),
  };
}
