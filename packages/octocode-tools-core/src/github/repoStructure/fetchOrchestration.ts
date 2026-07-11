import type { z } from 'zod';
import type { Octokit } from 'octokit';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type {
  GitHubRepositoryStructureResult,
  GitHubRepositoryStructureError,
} from '../../tools/github_view_repo_structure/types.js';
import { GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS } from '../../tools/github_view_repo_structure/constants.js';
import {
  getOctokit,
  resolveDefaultBranch,
  resolveCacheAuthFingerprint,
} from '../client.js';
import { handleGitHubAPIError } from '../errors.js';
import {
  generateCacheKey,
  withDataCacheConditional,
} from '../../utils/http/cache.js';
import { REPOSITORY_ERRORS } from '../../errors/domainErrors.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

import { applyStructurePagination } from '../repoStructurePagination.js';
import {
  fetchDirectoryContentsRecursivelyAPI,
  getRecursiveFetchFailureCount,
} from '../repoStructureRecursive.js';
import {
  fetchStructureViaGitTree,
  isGitStructureTreesEnabled,
} from '../repoStructureTree.js';

import {
  resolveContentWithBranchFallback,
  mapApiItems,
} from './contentResolution.js';
import { buildStructureResult } from './resultBuilder.js';

type GitHubStructureFetchQuery = GitHubViewRepoStructureQuery & {
  includeSizes?: boolean;
};

type StructureFetchOutcome = {
  result: GitHubRepositoryStructureResult | GitHubRepositoryStructureError;
  etag?: string;
  notModified?: boolean;
};

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
