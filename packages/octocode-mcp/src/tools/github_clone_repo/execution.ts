/**
 * Bulk execution handler for the githubCloneRepo tool.
 *
 * Orchestrates cloning / sparse-fetching of GitHub repositories and
 * returns structured results with actionable next-step hints.
 */

import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
import type { CloneRepoQuerySchema } from '@octocodeai/octocode-core/schemas';

type CloneRepoQuery = z.infer<typeof CloneRepoQuerySchema>;
import { getDirectorySizeBytes } from 'octocode-shared';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type PartialCloneRepoQuery = WithOptionalMeta<CloneRepoQuery>;
import { handleCatchError, createSuccessResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import {
  createLazyProviderContext,
  providerSupports,
} from '../providerExecution.js';
import { cloneRepo } from './cloneRepo.js';
/** Evidence-conditional cache marker; followups are covered by the tool description. */
const CACHE_HIT_HINT = 'Served from 24-hour cache.';

export async function executeCloneRepo(
  args: ToolExecutionArgs<PartialCloneRepoQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
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

          const baseHints: string[] = [];
          if (result.cached) baseHints.push(CACHE_HIT_HINT);

          return createSuccessResult(
            query,
            resultData,
            true,
            TOOL_NAMES.GITHUB_CLONE_REPO,
            {
              extraHints: baseHints,
              rawResponse: getDirectorySizeBytes(result.localPath),
              evidence: {
                kind: 'content',
                answerReady: true,
                confidence: 'high',
                complete: true,
                reason: result.sparse_path
                  ? 'Repository sparse checkout is available locally.'
                  : 'Repository full shallow clone is available locally.',
              },
            }
          );
        },
      }),
    {
      toolName: TOOL_NAMES.GITHUB_CLONE_REPO,
      keysPriority: ['resolvedBranch', 'localPath', 'cached', 'error'],
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
