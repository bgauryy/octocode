import type { AuthInfo } from '@modelcontextprotocol/server';
import type {
  ProviderResponse,
  RepoStructureQuery,
  RepoStructureResult,
} from '../types.js';

import { viewGitHubRepositoryStructureAPI } from '../../github/repoStructure.js';
import { getOctokit } from '../../github/client.js';

/**
 * Best-effort per-language byte breakdown via GitHub's `/languages` endpoint —
 * so an agent can answer "dominant implementation language" from a real
 * measurement instead of inferring it from repo structure. One extra API call,
 * opt-in via `includeLanguages`, and non-fatal on failure.
 */
async function fetchRepoLanguages(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<
  { languages: Record<string, number>; dominantLanguage: string } | undefined
> {
  try {
    const octokit = await getOctokit(authInfo);
    const resp = await octokit.rest.repos.listLanguages({ owner, repo });
    const entries = Object.entries(resp.data as Record<string, number>).sort(
      (a, b) => b[1] - a[1]
    );
    if (entries.length === 0) return undefined;
    return {
      languages: Object.fromEntries(entries),
      dominantLanguage: entries[0]![0],
    };
  } catch {
    return undefined;
  }
}

/** Top contributors by commit count (login + contributions), best-effort. */
async function fetchRepoContributors(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<
  { contributors: Array<{ login: string; contributions: number }> } | undefined
> {
  try {
    const octokit = await getOctokit(authInfo);
    const resp = await octokit.rest.repos.listContributors({
      owner,
      repo,
      per_page: 30,
    });
    const list = (
      resp.data as Array<{ login?: string; contributions?: number }>
    )
      .filter(c => typeof c.login === 'string')
      .map(c => ({ login: c.login!, contributions: c.contributions ?? 0 }));
    return list.length > 0 ? { contributors: list } : undefined;
  } catch {
    return undefined;
  }
}

/** Branch names (capped), best-effort. */
async function fetchRepoBranches(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<{ branches: string[] } | undefined> {
  try {
    const octokit = await getOctokit(authInfo);
    const resp = await octokit.rest.repos.listBranches({
      owner,
      repo,
      per_page: 100,
    });
    const names = (resp.data as Array<{ name?: string }>)
      .map(b => b.name)
      .filter((n): n is string => typeof n === 'string');
    return names.length > 0 ? { branches: names } : undefined;
  } catch {
    return undefined;
  }
}

/** Recent tags (name + sha), best-effort. */
async function fetchRepoTags(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<{ tags: Array<{ name: string; sha: string }> } | undefined> {
  try {
    const octokit = await getOctokit(authInfo);
    const resp = await octokit.rest.repos.listTags({
      owner,
      repo,
      per_page: 50,
    });
    const tags = (
      resp.data as Array<{ name?: string; commit?: { sha?: string } }>
    )
      .filter(t => typeof t.name === 'string')
      .map(t => ({ name: t.name!, sha: t.commit?.sha ?? '' }));
    return tags.length > 0 ? { tags } : undefined;
  } catch {
    return undefined;
  }
}

import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '../../toolContract/schemas.js';

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

  // Fetch the tree and every opt-in repo enrichment concurrently — one
  // round-trip regardless of how many enrichments were requested.
  const [result, languageInfo, contributorInfo, branchInfo, tagInfo] =
    await Promise.all([
      viewGitHubRepositoryStructureAPI(githubQuery, authInfo),
      query.includeLanguages
        ? fetchRepoLanguages(owner, repo, authInfo)
        : Promise.resolve(undefined),
      query.includeContributors
        ? fetchRepoContributors(owner, repo, authInfo)
        : Promise.resolve(undefined),
      query.includeBranches
        ? fetchRepoBranches(owner, repo, authInfo)
        : Promise.resolve(undefined),
      query.includeTags
        ? fetchRepoTags(owner, repo, authInfo)
        : Promise.resolve(undefined),
    ]);

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
    data: {
      ...transformRepoStructureResult(result),
      ...(languageInfo
        ? {
            languages: languageInfo.languages,
            dominantLanguage: languageInfo.dominantLanguage,
          }
        : {}),
      ...(contributorInfo
        ? { contributors: contributorInfo.contributors }
        : {}),
      ...(branchInfo ? { branches: branchInfo.branches } : {}),
      ...(tagInfo ? { tags: tagInfo.tags } : {}),
    },
    status: 200,
    provider: 'github',
    rawResponseChars: result.rawResponseChars ?? countSerializedChars(result),
  };
}
