import type { z } from 'zod';
import type { Octokit } from 'octokit';
import { AuthInfo } from '@modelcontextprotocol/server';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schema';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type {
  GitHubRepositoryStructureResult,
  GitHubRepositoryStructureError,
} from '../../tools/github_view_repo_structure/types.js';
import {
  GITHUB_STRUCTURE_DEFAULTS as STRUCTURE_DEFAULTS,
  CONTENTS_DIRECTORY_LIMIT,
} from '../../tools/github_view_repo_structure/constants.js';
import {
  getOctokit,
  resolveDefaultBranch,
  resolveCacheAuthFingerprint,
} from '../client.js';
import { handleGitHubAPIError } from '../errors.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCacheConditional } from '../../utils/http/cache/conditional.js';
import { REPOSITORY_ERRORS } from '../../errors/domainErrors.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

import { applyStructurePagination } from '../repoStructurePagination.js';
import {
  fetchDirectoryContentsRecursivelyAPI,
  getRecursiveFetchFailureCount,
  hasRecursiveContentsLimit,
  recoverContentsDirectory,
  shouldPropagateStructureError,
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
      includeSizes: (params as GitHubStructureFetchQuery).includeSizes === true,
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
      // A same-page retry must re-fetch failed subtrees instead of replaying
      // the incomplete cached listing.
      shouldCache: value => !('error' in value) && value.isPartial !== true,
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
    let contentsLimitReached = false;

    if (depth > 1) {
      // Reuse the resolved raw root; recursion owns its accounting and recovery.
      const recursiveItems = await fetchDirectoryContentsRecursivelyAPI(
        octokit,
        owner,
        repo,
        workingBranch,
        cleanPath,
        1,
        depth,
        undefined,
        { data }
      );
      partialTreeFailures = getRecursiveFetchFailureCount(recursiveItems);
      rawResponseChars = getRawResponseChars(recursiveItems) ?? 0;
      allItems = recursiveItems;
      contentsLimitReached = hasRecursiveContentsLimit(recursiveItems);
    } else {
      const recovered = await recoverContentsDirectory(
        octokit,
        { owner, repo, branch: workingBranch, path: cleanPath },
        rawItems.length,
        allItems
      );
      allItems = recovered.items;
      rawResponseChars += recovered.rawResponseChars;
      contentsLimitReached = recovered.contentsLimitReached;
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
        contentsLimitReached,
        rawResponseChars,
        includeSizes: params.includeSizes === true,
        itemsPerPage: params.itemsPerPage,
        page: params.page,
      }),
      // Soft ETag only for single-call depth-1 Contents (stable body ↔ etag).
      ...(depth === 1 && rawItems.length < CONTENTS_DIRECTORY_LIMIT && etag
        ? { etag }
        : {}),
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
        rateLimitRemaining: apiError.rateLimitRemaining,
        rateLimitReset: apiError.rateLimitReset,
        retryAfter: apiError.retryAfter,
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
    if (shouldPropagateStructureError(error)) throw error;
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
      depth,
      undefined,
      { data: resolution.data }
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
        contentsLimitReached: hasRecursiveContentsLimit(recursiveItems),
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
  let contentsLimitReached = false;
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
      contentsLimitReached = hasRecursiveContentsLimit(recursiveItems);
      rawResponseChars += getRawResponseChars(recursiveItems) ?? 0;
      const combined = [...allItems, ...recursiveItems];
      allItems = combined.filter(
        (item, index, array) =>
          array.findIndex(i => i.path === item.path) === index
      );
    } catch (error) {
      if (shouldPropagateStructureError(error)) throw error;
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
      contentsLimitReached,
      rawResponseChars,
      includeSizes: params.includeSizes === true,
      itemsPerPage: params.itemsPerPage,
      page: params.page,
      extraHints,
    }),
    ...(treeResult.etag && !incompleteTree ? { etag: treeResult.etag } : {}),
  };
}
