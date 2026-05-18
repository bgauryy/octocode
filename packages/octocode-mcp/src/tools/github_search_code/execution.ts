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

type PartialCodeSearchQuery = WithOptionalMeta<GitHubCodeSearchQuery>;
import { handleCatchError, createSuccessResult } from '../utils.js';
import {
  buildPaginationHints,
  mapCodeSearchProviderResult,
  mapCodeSearchToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';

export async function searchMultipleGitHubCode(
  args: ToolExecutionArgs<PartialCodeSearchQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialCodeSearchQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();

        const providerResult = await executeProviderOperation(query, () =>
          currentProviderContext.provider.searchCode(
            mapCodeSearchToolQuery(query)
          )
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        const result: GitHubSearchCodeData = mapCodeSearchProviderResult(
          providerResult.response.data,
          query
        );

        const hasContent = (result.files?.length || 0) > 0;
        const hasOwnerRepo = !!(query.owner && query.repo);
        const paginationHints = result.pagination
          ? buildPaginationHints(result.pagination, 'matches')
          : [];

        return createSuccessResult(
          query,
          result,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          {
            hintContext: { hasOwnerRepo, match: query.match },
            extraHints: paginationHints,
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      keysPriority: ['files', 'pagination', 'repositoryContext', 'error'],
      responseCharOffset,
      responseCharLength,
    }
  );
}
