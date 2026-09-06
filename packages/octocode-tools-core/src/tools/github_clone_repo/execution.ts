import type { CallToolResult } from '@modelcontextprotocol/server';
import { getCheckedOutSizeBytes } from './contentSize.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { markResponseCacheHit } from '../../utils/http/cache/trace.js';
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
import { getConfigSync } from '@octocodeai/config';

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
          if (getConfigSync().storage.mode === 'memory') {
            return createErrorResult(
              'Clone requires persistent local storage. Set storage.mode="persistent" or OCTOCODE_STORAGE_MODE=persistent to use ghCloneRepo.',
              query,
              { extra: { errorCode: 'persistentStorageDisabled' } }
            );
          }
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

          if (result.cached) markResponseCacheHit();

          const totalSize = getCheckedOutSizeBytes(result.localPath);

          const location: Record<string, unknown> = {
            kind: query.sparsePath ? 'tree' : 'repo',
            localPath: result.localPath,
            source: 'clone',
            cached: result.cached,
            commitSha: result.commitSha,
            verified: result.verified,
            // Completeness is relative to the requested clone scope. A sparse
            // checkout contains every reachable entry under sparsePath.
            complete: true,
            resolvedBranch: result.branch,
            ...(query.sparsePath ? { requestedPath: query.sparsePath } : {}),
          };

          // Tree orientation is ready to run without inventing a search term.
          const next: Record<string, unknown> = {
            viewStructure: {
              tool: 'localSearch',
              query: { operation: 'tree', path: result.localPath },
              why: 'Browse the cloned directory before choosing a text, structural, or file-discovery operation.',
              confidence: 'exact',
            },
          };

          const resultData: Record<string, unknown> = {
            owner: query.owner,
            repo: query.repo,
            // `location` is the canonical envelope for where content was saved
            // (localPath/resolvedBranch/cached/requestedPath live there); the
            // flat duplicates were dropped to avoid emitting each value twice.
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
      keysPriority: ['totalSize', 'location', 'error'],
      // Full and sparse clones have their own 120-second subprocess bound.
      // Let that operation finish (or report its typed failure) before the
      // shared bulk isolation layer aborts the query at its 60-second default.
      minQueryTimeoutMs: 130_000,
    },
    args
  );
}
