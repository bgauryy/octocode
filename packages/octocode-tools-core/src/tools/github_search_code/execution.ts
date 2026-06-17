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
import { buildGhSearchCodeFinalizer } from './finalizer.js';

type PartialCodeSearchQuery = WithOptionalMeta<GitHubCodeSearchQuery>;

function hasValidCodeSearchParams(query: PartialCodeSearchQuery): boolean {
  const keywords = query.keywords ?? [];
  return Boolean(
    keywords.some(keyword => keyword.trim().length > 0) ||
    query.owner ||
    query.path ||
    query.extension ||
    query.filename
  );
}

function validateCodeSearchScope(
  query: PartialCodeSearchQuery
): { error: string; hints: string[] } | undefined {
  if (query.repo && !query.owner) {
    return {
      error:
        'Repository scope requires owner. Provide both owner and repo, or omit repo for a broader search.',
      hints: [
        'Use owner="<org-or-user>" with repo="<repository>" — GitHub code search cannot scope to a bare repository name.',
        'If you only know the repo name, first use ghSearchRepos with keywords=["<repo>"] to find its owner.',
      ],
    };
  }
  return undefined;
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
        const scopeValidation = validateCodeSearchScope(query);
        if (scopeValidation) {
          return createErrorResult(scopeValidation.error, query, {
            customHints: scopeValidation.hints,
          });
        }

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
          keywords: query.keywords,
          totalMatches: flat.pagination?.totalMatches,
          hasMore: flat.pagination?.hasMore,
          currentPage: flat.pagination?.currentPage ?? 1,
          totalPages: flat.pagination?.totalPages ?? 1,
          matchedPaths: flat.results.flatMap(group =>
            group.matches.map(m => m.path)
          ),
        };
        const fileCount = new Set(
          flat.results.flatMap(group =>
            group.matches.map(
              match => `${group.owner}/${group.repo}:${match.path}`
            )
          )
        ).size;
        const successHints: string[] = [];
        if (flat.results.length > 0) {
          const firstKeyword =
            Array.isArray(query.keywords) &&
            typeof query.keywords[0] === 'string'
              ? query.keywords[0]
              : '<keyword>';
          successHints.push(
            `Found matches in ${fileCount} file${fileCount === 1 ? '' : 's'} — matchIndices[].lineOffset is the 0-based line within the snippet; use ghGetFileContent(path, matchString="${firstKeyword}") to land on the matched region (returns lineHint for lspGetSemantics).`
          );
        }
        if (flat.pagination) {
          const {
            totalPages,
            perPage,
            totalMatches,
            reportedTotalMatches,
            reachableTotalMatches,
          } = flat.pagination;
          const reported = reportedTotalMatches ?? totalMatches;
          const reachable =
            reachableTotalMatches ??
            Math.min(totalMatches, totalPages * perPage);
          if (reported > reachable) {
            successHints.push(
              `GitHub caps code-search at ${reachable} results — ${reported - reachable} of ${reported} reported matches are unreachable; narrow with path/extension/filename to see the rest.`
            );
          }
        }
        const pathLooksLikeFile =
          typeof query.path === 'string' &&
          !query.filename &&
          /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/.test(query.path);
        if (pathLooksLikeFile) {
          const extracted = query.path!.match(
            /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/
          );
          const fname = extracted ? extracted[1] : query.path;
          successHints.push(
            `path="${query.path}" looks like a file path — auto-extracted filename="${fname}" for the query. Use explicit filename="${fname}" + path="<dir>" for clarity.`
          );
        }
        return createSuccessResult(
          query,
          flat as GitHubSearchCodeData,
          flat.results.length > 0,
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          {
            hintContext,
            rawResponse: providerResult.response.rawResponseChars,
            extraHints: successHints,
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      peerHints: true,
      finalize: buildGhSearchCodeFinalizer<PartialCodeSearchQuery>(),
    },
    args
  );
}
