import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { getDirectorySizeBytes } from 'octocode-shared';
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

const CACHE_HIT_HINT = 'Served from 24-hour cache.';

const CLONE_FAILURE_HINTS = [
  'Verify the owner/repo (and branch) exist — use ghSearchRepos to confirm the repository name.',
  'For private repositories, ensure the GitHub token is set and has repo read access.',
];

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
              query,
              { customHints: CLONE_FAILURE_HINTS }
            );
          }

          const resultData: Record<string, unknown> = {
            owner: query.owner,
            repo: query.repo,
            localPath: result.localPath,
            ...(result.cached ? { cached: true } : {}),
            ...(query.branch !== result.branch
              ? { resolvedBranch: result.branch }
              : {}),
          };

          const baseHints: string[] = [];
          if (result.cached) baseHints.push(CACHE_HIT_HINT);
          baseHints.push(
            `Use localViewStructure with path="${result.localPath}" to explore, then localGetFileContent to read files.`
          );

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
      peerHints: true,
    },
    args
  );
}
