import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubSearchCodeData } from '@octocodeai/octocode-core/types';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';
import {
  createErrorResult,
  createSuccessResult,
  handleCatchError,
} from '../utils.js';
import {
  mapCodeSearchProviderResult,
  mapCodeSearchToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import { buildGithubSearchCodeFinalizer } from './finalizer.js';

export { applyGithubSearchCodeVerbosity } from './finalizer.js';

type PartialCodeSearchQuery = WithOptionalMeta<GitHubCodeSearchQuery>;

function hasValidCodeSearchParams(query: PartialCodeSearchQuery): boolean {
  const keywords = query.keywordsToSearch ?? [];
  return Boolean(
    keywords.some(keyword => keyword.trim().length > 0) ||
    query.owner ||
    query.repo ||
    query.path ||
    query.extension ||
    query.filename
  );
}

export async function searchMultipleGitHubCode(
  args: ToolExecutionArgs<PartialCodeSearchQuery>
): Promise<CallToolResult> {
  const { queries } = args;
  const getProviderContext = createLazyProviderContext(args.authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialCodeSearchQuery, _index: number) => {
      try {
        if (!hasValidCodeSearchParams(query)) {
          return createErrorResult(
            'At least one search term or scope filter is required.',
            query
          );
        }
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

        const hintContext = {
          hasOwnerRepo: Boolean(query.owner && query.repo),
          owner: query.owner,
          repo: query.repo,
          nonExistentScope: flat.nonExistentScope,
          match: query.match,
          extension: query.extension,
          filename: query.filename,
          path: query.path,
          keywords: query.keywordsToSearch,
          totalMatches: flat.pagination?.totalMatches,
          hasMore: flat.pagination?.hasMore,
          currentPage: flat.pagination?.currentPage ?? 1,
          totalPages: flat.pagination?.totalPages ?? 1,
          matchedPaths: flat.results.flatMap(group =>
            group.matches.map(m => m.path)
          ),
        };
        const fileCount = flat.results.flatMap(r => r.matches).length;
        return createSuccessResult(
          query,
          flat as unknown as GitHubSearchCodeData,
          flat.results.length > 0,
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          {
            hintContext,
            rawResponse: providerResult.response.rawResponseChars,
            extraHints:
              flat.results.length > 0
                ? [
                    `Found matches in ${fileCount} file${fileCount === 1 ? '' : 's'} — use githubGetFileContent(owner, repo, branch, path) to read specific files.`,
                  ]
                : [],
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      peerHints: true,
      peerEvidence: true,
      finalize: buildGithubSearchCodeFinalizer<PartialCodeSearchQuery>(),
    }
  );
}
