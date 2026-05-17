/**
 * Bulk execution handler for the githubCloneRepo tool.
 *
 * Orchestrates cloning / sparse-fetching of GitHub repositories and
 * returns structured results with actionable next-step hints.
 */

import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { CloneRepoQuery } from '@octocodeai/octocode-core';
import { getDirectorySizeBytes } from 'octocode-shared';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { handleCatchError, createSuccessResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import {
  createLazyProviderContext,
  providerSupports,
} from '../providerExecution.js';
import { cloneRepo } from './cloneRepo.js';
import {
  LOCAL_TOOL_LIST,
  LSP_TOOL_LIST,
} from '../../hints/localToolUsageHints.js';

/** Hints for full clones */
const FULL_CLONE_HINTS: string[] = [
  'Repository cloned locally (full, shallow depth=1).',
  'Use `localPath` as the `path` parameter for local tools:',
  ...LOCAL_TOOL_LIST,
  ...LSP_TOOL_LIST,
  'Tip: start with localViewStructure to understand the project layout.',
];

/** Hints for sparse (partial) checkouts */
const SPARSE_CLONE_HINTS: string[] = [
  'Partial tree fetched (sparse checkout – only the requested path was downloaded).',
  'Use `localPath` as the `path` parameter for local tools:',
  ...LOCAL_TOOL_LIST,
  'Note: LSP may have limited cross-file resolution in sparse checkouts.',
  'If you need full project context, re-clone without sparse_path.',
];

/** Hints for cached results */
const CACHE_HIT_HINT =
  'Served from 24-hour cache (no network call). To force refresh, set forceRefresh: true in the query.';

export async function executeCloneRepo(
  args: ToolExecutionArgs<CloneRepoQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: CloneRepoQuery, _index: number) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.GITHUB_CLONE_REPO,
        query,
        contextMessage: `Clone failed for ${query.owner}/${query.repo}`,
        execute: async () => {
          const providerContext = getProviderContext();

          if (!providerSupports(providerContext, 'cloneRepo')) {
            return handleCatchError(
              new Error(
                'githubCloneRepo is only available with the GitHub provider.'
              ),
              query,
              'Provider not supported',
              TOOL_NAMES.GITHUB_CLONE_REPO
            );
          }

          const result = await cloneRepo(
            query,
            authInfo,
            providerContext.token
          );

          const resultData: Record<string, unknown> = {
            localPath: result.localPath,
            ...(result.cached ? { cached: true } : {}),
            ...(query.branch !== result.branch
              ? { resolvedBranch: result.branch }
              : {}),
          };

          const baseHints = result.sparse_path
            ? [...SPARSE_CLONE_HINTS]
            : [...FULL_CLONE_HINTS];

          if (result.cached) {
            baseHints.unshift(CACHE_HIT_HINT);
          }

          return createSuccessResult(
            query,
            resultData,
            true,
            TOOL_NAMES.GITHUB_CLONE_REPO,
            {
              extraHints: baseHints,
              rawResponse: getDirectorySizeBytes(result.localPath),
            }
          );
        },
      }),
    {
      toolName: TOOL_NAMES.GITHUB_CLONE_REPO,
      keysPriority: ['resolvedBranch', 'localPath', 'cached', 'error'],
      responseCharOffset,
      responseCharLength,
    }
  );
}
