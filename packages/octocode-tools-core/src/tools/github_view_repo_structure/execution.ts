import type { CallToolResult } from '@modelcontextprotocol/server';
import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '../../toolContract/schemas.js';
import type {
  GitHubViewRepoStructureToolResult,
  GitHubRepoStructureDirectoryEntry,
} from '@octocodeai/octocode-core/extra-types';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type { WithOptionalMeta } from '../../types/execution.js';

type PartialRepoStructureQuery = WithOptionalMeta<GitHubViewRepoStructureQuery>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { shouldIgnoreFile, shouldIgnoreDir } from '../../utils/file/filters.js';
import { handleCatchError, createSuccessResult } from '../utils.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import {
  mapRepoStructureProviderResult,
  mapRepoStructureToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';

function normalizeStructureErrorResult(
  result: ProcessedBulkResult,
  query: PartialRepoStructureQuery
): ProcessedBulkResult {
  const rawError = result.error;
  const apiError =
    typeof rawError === 'object' && rawError !== null
      ? (rawError as { error?: unknown; status?: unknown; type?: unknown })
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
            tool: 'github.tree',
            query: {
              owner: query.owner,
              repo: query.repo,
              ...(parent ? { path: parent } : {}),
              ...(query.branch ? { branch: query.branch } : {}),
            },
            why: 'Retry at the parent directory — the path or branch may be wrong.',
            confidence: 'low',
          },
          searchPath: {
            tool: 'github.code',
            query: {
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
    if (dirPath !== '' && dirPath !== '.' && shouldIgnoreDir(dirName)) {
      continue;
    }

    const filteredFiles = entry.files.filter(
      fileName => !shouldIgnoreFile(fileName)
    );
    const filteredFolders = entry.folders.filter(
      folderName => !shouldIgnoreDir(folderName)
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
  args: ToolExecutionArgs<PartialRepoStructureQuery>,
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
    const hasContent = Object.keys(filteredStructure).length > 0;
    const resultData = mapRepoStructureProviderResult(
      providerResult.response.data,
      query,
      filteredStructure,
      effectiveBranch
    );
    if (branchFallbackWarning) {
      (resultData as Record<string, unknown>).branchFallback = {
        requestedBranch: explicitBranch,
        actualBranch: effectiveBranch,
        warning: branchFallbackWarning,
      };
    }

    // Ready-to-run follow-ups: read the first listed file, or materialize
    // the whole directory for local search/LSP.
    const structure = (
      resultData as {
        structure?: Array<{ dir: string; files?: string[] }>;
      }
    ).structure;
    const firstDir = structure?.find(d => (d.files?.length ?? 0) > 0);
    // `structure[].dir` is RELATIVE to the queried path, so a `fetchFile`
    // hint must re-prefix `query.path` (like `materialize` below does) —
    // otherwise it emits a bare filename that 404s for any non-root query.
    const structureBase = String(query.path ?? '').replace(/\/+$/, '');
    const firstFile = firstDir
      ? (() => {
          const rel =
            firstDir.dir === '.'
              ? firstDir.files![0]
              : `${firstDir.dir}/${firstDir.files![0]}`;
          return structureBase ? `${structureBase}/${rel}` : rel;
        })()
      : undefined;
    (resultData as Record<string, unknown>).next = {
      ...(firstFile
        ? {
            fetchFile: {
              tool: 'ghGetFileContent',
              query: {
                owner: query.owner,
                repo: query.repo,
                path: firstFile,
                // Use the branch actually served — after a fallback,
                // query.branch is the invalid requested ref.
                ...(effectiveBranch ? { branch: effectiveBranch } : {}),
              },
              why: 'Read the first listed file',
              confidence: 'low',
            },
          }
        : {}),
      materialize: {
        tool: 'ghGetFileContent',
        query: {
          owner: query.owner,
          repo: query.repo,
          path: String(query.path ?? ''),
          type: 'directory',
          ...(effectiveBranch ? { branch: effectiveBranch } : {}),
        },
        why: 'Materialize this directory locally for exact line anchors, local search, or LSP',
        confidence: 'exact',
      },
    };

    return createSuccessResult(
      query,
      resultData as unknown as Record<string, unknown>,
      hasContent,
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        rawResponse: providerResult.response.rawResponseChars,
      }
    );
  } catch (error) {
    return handleCatchError(
      error,
      query,
      'Failed to explore repository structure',
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );
  }
}

export async function exploreMultipleRepositoryStructures(
  args: ToolExecutionArgs<PartialRepoStructureQuery>
): Promise<CallToolResult> {
  const getProviderContext = createLazyProviderContext(args.authInfo);
  return executeBulkOperation(
    args.queries,
    query => exploreRepositoryStructure(query, args, getProviderContext),
    {
      toolName: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      keysPriority: [
        'resolvedBranch',
        'branchFallback',
        'summary',
        'pagination',
        'structure',
        'error',
      ] satisfies Array<keyof GitHubViewRepoStructureToolResult>,
    },
    args
  );
}
