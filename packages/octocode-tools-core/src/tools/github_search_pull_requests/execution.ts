import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubSearchPullRequestsToolResult } from '@octocodeai/octocode-core/extra-types';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type GitHubPullRequestSearchInput = z.input<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;
type PartialPRQuery = WithOptionalMeta<GitHubPullRequestSearchQuery>;
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
  safeParseOrError,
} from '../utils.js';
import {
  mapPullRequestProviderResultData,
  mapPullRequestToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import { normalizePullRequestContentRequest } from './contentRequest.js';
import { shapePullRequestForContent } from './contentResponse.js';
import { fetchHistory } from '../../github/history.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<GitHubPullRequestSearchInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchInput, _index: number) => {
      try {
        const parsed = safeParseOrError(
          GitHubPullRequestSearchQueryLocalSchema,
          query
        );
        if (parsed.ok === false) {
          return parsed.error;
        }

        // --- commits mode: route to commit history API ---
        if ((parsed.data as { type?: string }).type === 'commits') {
          const q = parsed.data as {
            type?: string;
            owner?: string;
            repo?: string;
            path?: string;
            branch?: string;
            author?: string;
            since?: string;
            until?: string;
            page?: number;
            perPage?: number;
            filePage?: number;
            itemsPerPage?: number;
            includeDiff?: boolean;
            charOffset?: number;
            charLength?: number;
          };

          if (!q.owner || !q.repo) {
            return createErrorResult(
              'owner and repo are required for commits mode.',
              query
            );
          }

          const path = q.path;
          // A path ending in '/' is a directory prefix → repo mode; a specific file path → file mode
          const historyType = path && !path.endsWith('/') ? 'file' : 'repo';

          if (historyType === 'file' && !path) {
            return createErrorResult(
              'path is required when querying a specific file in commits mode.',
              query
            );
          }

          const result = await fetchHistory(
            {
              type: historyType,
              owner: q.owner,
              repo: q.repo,
              path,
              branch: q.branch,
              since: q.since,
              until: q.until,
              author: q.author,
              page: Number(q.page) || 1,
              perPage: Number(q.perPage) || 30,
              filePage: typeof q.filePage === 'number' ? q.filePage : undefined,
              itemsPerPage:
                typeof q.itemsPerPage === 'number' ? q.itemsPerPage : undefined,
              includeDiff: Boolean(q.includeDiff),
              charOffset:
                typeof q.charOffset === 'number' ? q.charOffset : undefined,
              charLength:
                typeof q.charLength === 'number' ? q.charLength : undefined,
            },
            authInfo
          );

          if (isGitHubAPIError(result)) {
            return createErrorResult(result, query, {
              toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
            });
          }

          const { commits } = result.data;
          const hasContent = commits.length > 0;

          return createSuccessResult(
            query,
            result.data as unknown as Record<string, unknown>,
            hasContent,
            TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
            {
              rawResponse: result.rawResponseChars,
            }
          );
        }
        // --- end commits mode ---

        const currentProviderContext = getProviderContext();
        const effectiveQuery: PartialPRQuery = { ...parsed.data };
        const contentRequest = normalizePullRequestContentRequest(
          effectiveQuery as never
        );
        const hasPrNumber = effectiveQuery.prNumber !== undefined;

        if (!hasPrNumber) {
          (effectiveQuery as { content?: unknown }).content = undefined;
          (effectiveQuery as { reviewMode?: unknown }).reviewMode = undefined;
        }

        const hasValidParams =
          effectiveQuery.keywordsToSearch?.length ||
          effectiveQuery.owner ||
          effectiveQuery.repo ||
          effectiveQuery.author ||
          effectiveQuery.assignee ||
          (effectiveQuery.prNumber &&
            effectiveQuery.owner &&
            effectiveQuery.repo);

        if (!hasValidParams) {
          return createErrorResult(
            'At least one valid search parameter, filter, or PR number is required.',
            query
          );
        }

        const providerResult = await executeProviderOperation(
          effectiveQuery,
          () =>
            currentProviderContext.provider.searchPullRequests(
              mapPullRequestToolQuery(effectiveQuery)
            )
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        const includeFileChanges = hasPrNumber
          ? contentRequest.changedFiles ||
            contentRequest.patches.mode !== 'none'
          : false;
        const { pullRequests, resultData } = mapPullRequestProviderResultData(
          providerResult.response.data,
          {
            includeFileChanges,
          }
        );

        if (effectiveQuery.prNumber !== undefined) {
          delete (resultData as Record<string, unknown>).pagination;
        }

        const shouldLeanBroadShape =
          !hasPrNumber &&
          (Boolean((query as { content?: unknown }).content) ||
            Boolean((query as { reviewMode?: unknown }).reviewMode));
        const leanRequest = {
          ...contentRequest,
          body: false,
          changedFiles: false,
          patches: { mode: 'none' as const },
          comments: false as const,
          commits: false as const,
        };
        const shouldMinify =
          (effectiveQuery as { minify?: string }).minify === 'standard';
        const showContentMap = hasPrNumber;
        const shapedPullRequests = pullRequests.map(pr =>
          shapePullRequestForContent(
            pr,
            effectiveQuery as never,
            shouldLeanBroadShape ? leanRequest : contentRequest,
            shouldMinify,
            showContentMap
          )
        );
        resultData.pull_requests = shapedPullRequests;

        if (
          !hasPrNumber &&
          (effectiveQuery as { concise?: boolean }).concise === true
        ) {
          resultData.pull_requests = shapedPullRequests.map(pr => {
            const p = pr as { number?: unknown; title?: unknown };
            return `#${p.number} ${p.title}`;
          }) as unknown as typeof resultData.pull_requests;
        }

        const hasContent = shapedPullRequests.length > 0;

        // Per-call result/file-change/matchString hints were computed only from
        // populated results and dropped centrally by createSuccessResult on the
        return createSuccessResult(
          effectiveQuery,
          resultData as unknown as Record<string, unknown>,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
          {
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      keysPriority: [
        'pull_requests',
        'pagination',
        'total_count',
        'error',
      ] satisfies Array<keyof GitHubSearchPullRequestsToolResult>,
    },
    args
  );
}
