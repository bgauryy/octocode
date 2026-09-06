import type { AuthInfo } from '@modelcontextprotocol/server';
import type { ProviderResponse } from '../types.js';
import type { RepoStructureQuery } from '../providerQueries.js';
import type { RepoStructureResult } from '../providerResults.js';

import { viewGitHubRepositoryStructureAPI } from '../../github/repoStructure/fetchOrchestration.js';
import {
  getOctokit,
  resolveCacheAuthFingerprint,
} from '../../github/client.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCache } from '../../utils/http/cache/dataCache.js';
import { MAX_PAGE_NUMBER } from '../../toolContract/input/resources/tools/_toolkit.js';

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
): Promise<{
  languages?: Record<string, number>;
  dominantLanguage?: string;
  rawResponseChars: number;
  metadataPagination?: RepoStructureResult['metadataPagination'];
}> {
  try {
    const auth = await resolveCacheAuthFingerprint(authInfo);
    const key = generateCacheKey('gh-repo-metadata', {
      owner,
      repo,
      kind: 'languages',
      auth,
    });
    return await withDataCache(
      key,
      async () => {
        const octokit = await getOctokit(authInfo);
        const resp = await octokit.rest.repos.listLanguages({ owner, repo });
        const entries = Object.entries(
          resp.data as Record<string, number>
        ).sort((a, b) => b[1] - a[1]);
        return {
          languages: Object.fromEntries(entries),
          dominantLanguage: entries[0]?.[0],
          rawResponseChars: countSerializedChars(resp.data),
        };
      },
      { cacheRole: 'helper' }
    );
  } catch {
    return {
      rawResponseChars: 0,
      metadataPagination: {
        languages: {
          currentPage: 1,
          perPage: 1,
          returned: 0,
          hasMore: false,
          failed: true,
        },
      },
    };
  }
}

type MetadataKind = 'contributors' | 'branches' | 'tags';
type MetadataPage = NonNullable<
  RepoStructureResult['metadataPagination']
>[MetadataKind];

/** One bounded page. Link headers, not list length, determine continuation. */
async function fetchRepoMetadata(
  kind: MetadataKind,
  owner: string,
  repo: string,
  page: number,
  authInfo?: AuthInfo
): Promise<{
  contributors?: Array<{ login: string; contributions: number }>;
  branches?: string[];
  tags?: Array<{ name: string; sha: string }>;
  metadataPagination: Partial<Record<MetadataKind, MetadataPage>>;
  rawResponseChars: number;
}> {
  const perPage = { contributors: 30, branches: 100, tags: 50 }[kind];
  try {
    const auth = await resolveCacheAuthFingerprint(authInfo);
    const cacheKey = generateCacheKey('gh-repo-metadata', {
      owner,
      repo,
      kind,
      page,
      perPage,
      auth,
    });
    return await withDataCache(
      cacheKey,
      async () => {
        const octokit = await getOctokit(authInfo);
        const params = { owner, repo, page, per_page: perPage };
        const response = await (kind === 'contributors'
          ? octokit.rest.repos.listContributors(params)
          : kind === 'branches'
            ? octokit.rest.repos.listBranches(params)
            : octokit.rest.repos.listTags(params));
        const link = Object.entries(response.headers ?? {}).find(
          ([name]) => name.toLowerCase() === 'link'
        )?.[1];
        const hasMore =
          typeof link === 'string' && /rel=["']next["']/.test(link);
        const items = response.data;
        const data =
          kind === 'contributors'
            ? {
                contributors: (
                  items as Array<{ login?: string; contributions?: number }>
                )
                  .filter(c => typeof c.login === 'string')
                  .map(c => ({
                    login: c.login!,
                    contributions: c.contributions ?? 0,
                  })),
              }
            : kind === 'branches'
              ? {
                  branches: (items as Array<{ name: string }>).map(b => b.name),
                }
              : {
                  tags: (
                    items as Array<{ name: string; commit: { sha: string } }>
                  ).map(t => ({ name: t.name, sha: t.commit.sha })),
                };
        return {
          ...data,
          rawResponseChars: countSerializedChars(items),
          metadataPagination: {
            [kind]: {
              currentPage: page,
              perPage,
              returned: Object.values(data)[0]!.length,
              hasMore,
              ...(hasMore && page >= MAX_PAGE_NUMBER
                ? { terminalLimit: true }
                : {}),
            },
          },
        };
      },
      { cacheRole: 'helper' }
    );
  } catch {
    return {
      rawResponseChars: 0,
      metadataPagination: {
        [kind]: {
          currentPage: page,
          perPage,
          returned: 0,
          hasMore: false,
          failed: true,
        },
      },
    };
  }
}

import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '../../toolContract/input/resources/tools/githubTreeOperation.js';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type { GitHubRepositoryStructureResult } from '../../tools/github_view_repo_structure/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import {
  createGitHubProviderErrorFromResult,
  parseGitHubProjectId,
} from './utils.js';

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
      ...(data.summary?.incompleteTree !== undefined && {
        incompleteTree: data.summary.incompleteTree,
      }),
    },
    ...(data.isPartial !== undefined && { isPartial: data.isPartial }),
    ...(data.partialReasons !== undefined && {
      partialReasons: data.partialReasons,
    }),
    ...(data.terminalLimit !== undefined && {
      terminalLimit: data.terminalLimit,
    }),
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
    goal: query.goal,
    reasoning: query.reasoning,
  } as GitHubViewRepoStructureQuery & { includeSizes?: boolean };

  // Fetch the tree and each opt-in enrichment concurrently. Each uncached
  // enrichment makes its own API request; metadata caches span tree pages.
  const [result, languageInfo, contributorInfo, branchInfo, tagInfo] =
    await Promise.all([
      viewGitHubRepositoryStructureAPI(githubQuery, authInfo),
      query.includeLanguages
        ? fetchRepoLanguages(owner, repo, authInfo)
        : Promise.resolve(undefined),
      query.includeContributors
        ? fetchRepoMetadata(
            'contributors',
            owner,
            repo,
            query.metadataPage ?? 1,
            authInfo
          )
        : Promise.resolve(undefined),
      query.includeBranches
        ? fetchRepoMetadata(
            'branches',
            owner,
            repo,
            query.metadataPage ?? 1,
            authInfo
          )
        : Promise.resolve(undefined),
      query.includeTags
        ? fetchRepoMetadata(
            'tags',
            owner,
            repo,
            query.metadataPage ?? 1,
            authInfo
          )
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

  const transformed = transformRepoStructureResult(result);
  const metadataPagination: NonNullable<
    RepoStructureResult['metadataPagination']
  > = {
    ...contributorInfo?.metadataPagination,
    ...branchInfo?.metadataPagination,
    ...tagInfo?.metadataPagination,
    ...languageInfo?.metadataPagination,
  };
  const pages: MetadataPage[] = Object.values(metadataPagination);
  const partialReasons = new Set(transformed.partialReasons);
  for (const metadata of pages) {
    if (metadata?.failed) partialReasons.add('metadataFetchFailed');
    else if (metadata?.terminalLimit) partialReasons.add('metadataPageLimit');
    else if (metadata?.hasMore) partialReasons.add('metadataPagination');
  }

  return {
    data: {
      ...transformed,
      ...(languageInfo?.languages
        ? {
            languages: languageInfo.languages,
            dominantLanguage: languageInfo.dominantLanguage,
          }
        : {}),
      ...(contributorInfo?.contributors
        ? { contributors: contributorInfo.contributors }
        : {}),
      ...(branchInfo?.branches ? { branches: branchInfo.branches } : {}),
      ...(tagInfo?.tags ? { tags: tagInfo.tags } : {}),
      ...(pages.length > 0 ? { metadataPagination } : {}),
      ...(partialReasons.size > 0
        ? { isPartial: true, partialReasons: [...partialReasons] }
        : {}),
      ...(pages.some(metadata => metadata?.terminalLimit)
        ? { terminalLimit: true }
        : {}),
    },
    status: 200,
    provider: 'github',
    rawResponseChars:
      (result.rawResponseChars ?? countSerializedChars(result)) +
      (contributorInfo?.rawResponseChars ?? 0) +
      (branchInfo?.rawResponseChars ?? 0) +
      (tagInfo?.rawResponseChars ?? 0) +
      (languageInfo?.rawResponseChars ?? 0),
  };
}
