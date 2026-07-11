import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
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

export async function exploreMultipleRepositoryStructures(
  args: ToolExecutionArgs<PartialRepoStructureQuery>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialRepoStructureQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();
        const projectId = `${query.owner}/${query.repo}`;
        const explicitBranch = query.branch;
        const resolvedBranch =
          explicitBranch ??
          (await currentProviderContext.provider.resolveDefaultBranch(
            projectId
          ));

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
              await currentProviderContext.provider.resolveDefaultBranch(
                projectId
              );
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
        const firstFile = firstDir
          ? firstDir.dir === '.'
            ? firstDir.files![0]
            : `${firstDir.dir}/${firstDir.files![0]}`
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
                    ...(query.branch ? { branch: query.branch } : {}),
                  },
                  why: 'Read the first listed file',
                  confidence: 'heuristic',
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
              ...(query.branch ? { branch: query.branch } : {}),
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
    },
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
