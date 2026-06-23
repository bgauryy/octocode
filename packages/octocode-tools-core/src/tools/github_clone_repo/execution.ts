import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { getDirectorySizeBytes } from '../../shared/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type PartialCloneRepoQuery = WithOptionalMeta<CloneRepoQuery>;
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
} from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import {
  createLazyProviderContext,
  providerSupports,
} from '../providerExecution.js';
import { cloneRepo } from './cloneRepo.js';
import type { CloneRepoQueryLocalSchema } from './scheme.js';
import type { z } from 'zod';

type CloneRepoQuery = z.infer<typeof CloneRepoQueryLocalSchema>;

export async function executeCloneRepo(
  args: ToolExecutionArgs<PartialCloneRepoQuery>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialCloneRepoQuery, _index: number) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.GITHUB_CLONE_REPO,
        query,
        contextMessage: `Clone failed for ${query.owner}/${query.repo}`,
        execute: async () => {
          const providerContext = getProviderContext();

          if (!providerSupports(providerContext, 'cloneRepo')) {
            return handleCatchError(
              new Error(
                'ghCloneRepo is only available with the GitHub provider.'
              ),
              query,
              'Provider not supported',
              TOOL_NAMES.GITHUB_CLONE_REPO
            );
          }

          let result;
          try {
            result = await cloneRepo(query, authInfo, providerContext.token);
          } catch (error) {
            const message =
              error instanceof Error ? error.message : String(error);
            return createErrorResult(
              `Clone failed for ${query.owner}/${query.repo}: ${message}`,
              query
            );
          }

          const totalSize = getDirectorySizeBytes(result.localPath);

          const location: Record<string, unknown> = {
            kind: query.sparsePath ? 'tree' : 'repo',
            localPath: result.localPath,
            repoRoot: result.localPath,
            source: 'clone',
            cached: result.cached,
            complete: !query.sparsePath,
            resolvedBranch: result.branch,
            ...(query.sparsePath ? { requestedPath: query.sparsePath } : {}),
          };

          const next: Record<string, unknown> = {
            localSearch: {
              tool: 'localSearchCode',
              query: {
                path: result.localPath,
                mode: 'discovery',
              },
            },
            viewStructure: {
              tool: 'localViewStructure',
              query: { path: result.localPath },
            },
          };

          const resultData: Record<string, unknown> = {
            owner: query.owner,
            repo: query.repo,
            localPath: result.localPath,
            resolvedBranch: result.branch,
            cached: result.cached,
            ...(query.sparsePath ? { sparsePath: query.sparsePath } : {}),
            totalSize,
            location,
            next,
          };

          // Always a content result (hasContent=true); per-call next-step
          // hints are dropped centrally by createSuccessResult on success.
          return createSuccessResult(
            query,
            resultData,
            true,
            TOOL_NAMES.GITHUB_CLONE_REPO,
            {
              rawResponse: totalSize,
            }
          );
        },
      }),
    {
      toolName: TOOL_NAMES.GITHUB_CLONE_REPO,
      keysPriority: [
        'localPath',
        'resolvedBranch',
        'cached',
        'sparsePath',
        'totalSize',
        'fileCount',
        'location',
        'error',
      ],
    },
    args
  );
}
