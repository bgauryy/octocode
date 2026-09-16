import {
  GITHUB_SEARCH_TOOL_NAME,
  type GitHubSearchQuery,
} from '@octocodeai/octocode-core/schema';
import type { GitHubRepoStructureDirectoryEntry } from '@octocodeai/octocode-core/extra-types';

type GitHubViewRepoStructureQuery = Extract<
  GitHubSearchQuery,
  { operation: 'tree' }
>;
import type { WithOptionalMeta } from '../../types/execution.js';

type PartialRepoStructureQuery = WithOptionalMeta<GitHubViewRepoStructureQuery>;
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  shouldIgnoreDiscoveryFile,
  shouldIgnoreDiscoveryDir,
} from '@octocodeai/octocode-engine/security';
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
} from '../utils.js';
import { handleGitHubAPIError } from '../../github/errors.js';
import type { GitHubAPIError } from '../../github/githubAPI.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import {
  mapRepoStructureProviderResult,
  mapRepoStructureToolQuery,
} from '../providerMappers/repoStructure.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import { fetchDirectoryContents } from '../../github/directoryFetch/fetchDirectoryContents.js';

function normalizeStructureErrorResult(
  result: ProcessedBulkResult,
  query: PartialRepoStructureQuery
): ProcessedBulkResult {
  const rawError = result.error;
  const apiError =
    typeof rawError === 'object' && rawError !== null
      ? (rawError as Partial<GitHubAPIError>)
      : undefined;

  const status =
    typeof apiError?.status === 'number' ? apiError.status : undefined;

  // On a 404, hand back a structured recovery (mirrors ghGetFileContent's
  // file-404): retry at the parent dir (deterministic) and/or locate the path
  // by name in case it moved/renamed — instead of a dead-end error.
  const cleanPath =
    typeof query.path === 'string' ? query.path.replace(/\/+$/, '') : '';
  const parent = cleanPath.includes('/')
    ? cleanPath.slice(0, cleanPath.lastIndexOf('/'))
    : '';
  const leaf = cleanPath.split('/').pop() || query.repo;
  const next =
    status === 404
      ? {
          retryParent: {
            tool: 'ghSearch',
            query: {
              operation: 'tree',
              owner: query.owner,
              repo: query.repo,
              ...(parent ? { path: parent } : {}),
              ...(query.branch ? { branch: query.branch } : {}),
            },
            why: 'Retry at the parent directory — the path or branch may be wrong.',
            confidence: 'low',
          },
          searchPath: {
            tool: 'ghSearch',
            query: {
              operation: 'code',
              owner: query.owner,
              repo: query.repo,
              match: 'path',
              keywords: [leaf],
            },
            why: 'Locate the path by name in case it moved or was renamed.',
            confidence: 'low',
          },
        }
      : undefined;

  return {
    status: 'error',
    owner: query.owner,
    repo: query.repo,
    path: query.path,
    branch: query.branch,
    error:
      typeof apiError?.error === 'string'
        ? apiError.error
        : typeof rawError === 'string'
          ? rawError
          : 'Failed to explore repository structure',
    ...(typeof apiError?.status === 'number'
      ? { statusCode: apiError.status }
      : {}),
    ...(typeof apiError?.type === 'string' ? { errorType: apiError.type } : {}),
    ...(typeof apiError?.retryAfter === 'number'
      ? { retryAfter: apiError.retryAfter }
      : {}),
    ...(typeof apiError?.rateLimitRemaining === 'number'
      ? { rateLimitRemaining: apiError.rateLimitRemaining }
      : {}),
    ...(typeof apiError?.rateLimitReset === 'number'
      ? { rateLimitReset: apiError.rateLimitReset }
      : {}),
    ...(next ? { next } : {}),
  };
}

export function filterStructure(
  structure: Record<string, GitHubRepoStructureDirectoryEntry>
): Record<string, GitHubRepoStructureDirectoryEntry> {
  const filtered: Record<string, GitHubRepoStructureDirectoryEntry> = {};

  for (const [dirPath, entry] of Object.entries(structure)) {
    // Skip top-level entries for directories that should be ignored
    const dirName = dirPath.split('/').pop() ?? dirPath;
    if (
      dirPath !== '' &&
      dirPath !== '.' &&
      shouldIgnoreDiscoveryDir(dirName)
    ) {
      continue;
    }

    const filteredFiles = entry.files.filter(
      fileName => !shouldIgnoreDiscoveryFile(fileName)
    );
    const filteredFolders = entry.folders.filter(
      folderName => !shouldIgnoreDiscoveryDir(folderName)
    );

    if (filteredFiles.length > 0 || filteredFolders.length > 0) {
      filtered[dirPath] = {
        files: filteredFiles,
        folders: filteredFolders,
      };
    }
  }

  return filtered;
}

export async function exploreRepositoryStructure(
  query: PartialRepoStructureQuery,
  args: ToolExecutionArgs<GitHubSearchQuery>,
  getProviderContext = createLazyProviderContext(args.authInfo)
): Promise<ProcessedBulkResult> {
  try {
    const currentProviderContext = getProviderContext();
    const projectId = `${query.owner}/${query.repo}`;
    const explicitBranch = query.branch;
    const resolvedBranch =
      explicitBranch ??
      (await currentProviderContext.provider.resolveDefaultBranch(projectId));

    let providerResult = await executeProviderOperation(query, () =>
      currentProviderContext.provider.getRepoStructure(
        mapRepoStructureToolQuery(query, resolvedBranch)
      )
    );

    let effectiveBranch = resolvedBranch;
    let branchFallbackWarning: string | undefined;

    // The schema documents that an unresolvable ref falls back to the
    // default branch with a warning — but that only ever worked when
    // `branch` was omitted (resolved upfront, above). An EXPLICIT bad
    // branch 404s outright with no retry, contradicting the documented
    // contract. Retry once against the actual default branch so the
    // fallback promise holds for explicit branches too.
    if (providerResult.ok === false && explicitBranch) {
      const rawError = providerResult.result.error;
      const status =
        typeof rawError === 'object' && rawError !== null
          ? (rawError as { status?: unknown }).status
          : undefined;
      if (status === 404) {
        const defaultBranch =
          await currentProviderContext.provider.resolveDefaultBranch(projectId);
        if (defaultBranch !== explicitBranch) {
          const retryResult = await executeProviderOperation(query, () =>
            currentProviderContext.provider.getRepoStructure(
              mapRepoStructureToolQuery(query, defaultBranch)
            )
          );
          if (retryResult.ok !== false) {
            providerResult = retryResult;
            effectiveBranch = defaultBranch;
            branchFallbackWarning = `Branch/ref '${explicitBranch}' was not found. Showing '${defaultBranch}' (default branch) instead. Re-query with the correct branch name if branch-specific results are required.`;
          }
        }
      }
    }

    if (providerResult.ok === false) {
      return normalizeStructureErrorResult(providerResult.result, query);
    }

    const filteredStructure = filterStructure(
      providerResult.response.data.structure
    );
    const resultData = mapRepoStructureProviderResult(
      providerResult.response.data,
      query,
      filteredStructure,
      effectiveBranch
    );
    const hasContent =
      Object.keys(filteredStructure).length > 0 ||
      resultData.isPartial === true ||
      resultData.languages !== undefined ||
      ['contributors', 'branches', 'tags'].some(
        key => Array.isArray(resultData[key]) && resultData[key].length > 0
      );
    if (branchFallbackWarning) {
      (resultData as Record<string, unknown>).branchFallback = {
        requestedBranch: explicitBranch,
        actualBranch: effectiveBranch,
        warning: branchFallbackWarning,
      };
    }

    const materialize = (query as { materialize?: boolean }).materialize === true;
    if (materialize) {
      if (
        typeof query.owner !== 'string' ||
        query.owner.length === 0 ||
        typeof query.repo !== 'string' ||
        query.repo.length === 0
      ) {
        throw new Error(
          'GitHub repository owner and name are required for materialization'
        );
      }
      const snapshot = await fetchDirectoryContents(
        query.owner,
        query.repo,
        typeof query.path === 'string' ? query.path : '',
        effectiveBranch,
        args.authInfo
      ).catch(() => null);
      if (snapshot) {
      const offset =
        (query as { materializeOffset?: number }).materializeOffset ?? 0;
      (resultData as Record<string, unknown>).location = {
        kind: 'local',
        localPath: snapshot.localPath,
        source: 'github-tree',
        cached: snapshot.cached,
        complete: snapshot.complete,
        hasMore: !snapshot.complete,
        resolvedBranch: effectiveBranch,
        commitSha: snapshot.commitSha,
      };
      if (!snapshot.complete) {
        const nextQuery = {
          operation: 'tree' as const,
          owner: query.owner,
          repo: query.repo,
          branch: effectiveBranch,
          ...(query.path ? { path: query.path } : {}),
          materialize: true,
          materializeOffset: offset + snapshot.savedFileCount,
        };
        const next = {
          ...((resultData as { next?: Record<string, unknown> }).next ?? {}),
        };
        delete next.nextPage;
        next.continueMaterialize = {
          tool: GITHUB_SEARCH_TOOL_NAME,
          query: nextQuery,
          why: 'Continue writing tree files after the per-call write cap.',
          confidence: 'exact',
        };
        (resultData as Record<string, unknown>).next = next;
      }
      }
    }

    return createSuccessResult(
      query,
      resultData as unknown as Record<string, unknown>,
      hasContent,
      GITHUB_SEARCH_TOOL_NAME,
      {
        rawResponse: providerResult.response.rawResponseChars,
      }
    );
  } catch (error) {
    const apiError = handleGitHubAPIError(error);
    if (apiError.type === 'http') {
      return normalizeStructureErrorResult(
        createErrorResult(apiError, query),
        query
      );
    }
    return handleCatchError(
      error,
      query,
      'Failed to explore repository structure',
      GITHUB_SEARCH_TOOL_NAME
    );
  }
}
