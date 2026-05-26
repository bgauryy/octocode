import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type {
  GitHubCodeSearchQuery,
  GitHubSearchCodeData,
} from '@octocodeai/octocode-core';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';
import { createSuccessResult, handleCatchError } from '../utils.js';
import {
  mapCodeSearchProviderResult,
  mapCodeSearchToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import { buildGithubSearchCodeFinalizer } from './finalizer.js';

type PartialCodeSearchQuery = WithOptionalMeta<GitHubCodeSearchQuery>;

export async function searchMultipleGitHubCode(
  args: ToolExecutionArgs<PartialCodeSearchQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength, format } = args;
  const getProviderContext = createLazyProviderContext(args.authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialCodeSearchQuery, _index: number) => {
      try {
        const ctx = getProviderContext();
        const providerResult = await executeProviderOperation(query, () =>
          ctx.provider.searchCode(mapCodeSearchToolQuery(query))
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        const flat = mapCodeSearchProviderResult(
          providerResult.response.data,
          query
        );

        // We stash the flat per-query shape into the standard tool data
        // surface; the finalizer reads it back and reshapes the whole bulk.
        // Cast through `unknown` since the upstream type expects the legacy
        // {files, pagination} shape — this local schema is overridden in
        // GitHubCodeSearchOutputLocalSchema.
        // Query-shape context lets per-tool hints.ts pick the most specific
        // empty-result recovery line — naming the filters in play, suggesting
        // which to drop, calling out the AND-logic gotcha.
        const hintContext = {
          hasOwnerRepo: Boolean(query.owner && query.repo),
          owner: query.owner,
          repo: query.repo,
          match: query.match,
          extension: query.extension,
          filename: query.filename,
          path: query.path,
          keywords: query.keywordsToSearch,
          // Pagination signals so hasResults hint can emit exhaustive-search guidance
          totalMatches: flat.pagination?.totalMatches,
          hasMore: flat.pagination?.hasMore,
          currentPage: flat.pagination?.currentPage ?? 1,
          totalPages: flat.pagination?.totalPages ?? 1,
          // Matched paths drive the non-canonical (examples/__tests__/docs)
          // concept-match warning in hints.hasResults.
          matchedPaths: flat.results.flatMap(group =>
            group.matches.map(m => m.path)
          ),
        };
        return createSuccessResult(
          query,
          flat as unknown as GitHubSearchCodeData,
          flat.results.length > 0,
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          {
            hintContext,
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      responseCharOffset,
      responseCharLength,
      format,
      finalize: buildGithubSearchCodeFinalizer<PartialCodeSearchQuery>(),
    }
  );
}
