import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
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
import {
  buildGithubSearchCodeFinalizer,
  CONCISE_SEARCH_CODE_LIMIT,
} from './finalizer.js';
import { isConcise } from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';

// Re-exported so every tool exposes `apply<Tool>Verbosity` from execution.ts.
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
  const { queries, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(args.authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialCodeSearchQuery, _index: number) => {
      try {
        // Pre-flight: cap the effective page size under concise so the upstream
        // fetch reflects concise's documented "capped at 3" probe contract.
        // Capping here (not just in the finalizer) ensures the provider returns
        // at most 3 files instead of trimming after fetch — matching the
        // githubSearchRepositories pattern. The finalizer's group/value shaping
        // still applies under the all-concise gate.
        const verbosityIsConcise = isConcise(
          (query as WithVerbosity<typeof query>).verbosity
        );
        if (verbosityIsConcise) {
          // Cap the effective per_page to the concise probe size by capping
          // BOTH knobs the per_page resolver reads (itemsPerPage drives
          // per_page; githubAPILimit overrides it when present).
          const q = query as {
            itemsPerPage?: number;
            githubAPILimit?: number;
          };
          q.itemsPerPage = Math.min(
            q.itemsPerPage ?? CONCISE_SEARCH_CODE_LIMIT,
            CONCISE_SEARCH_CODE_LIMIT
          );
          if (typeof q.githubAPILimit === 'number') {
            q.githubAPILimit = Math.min(
              q.githubAPILimit,
              CONCISE_SEARCH_CODE_LIMIT
            );
          }
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
          // GitHub reported the owner/repo/user does not exist (422) — distinct
          // from a valid scope that matched nothing. Drives a scope-spelling
          // hint instead of authoritative "not found".
          nonExistentScope: flat.nonExistentScope,
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
      peerHints: true,
      peerEvidence: true,
      finalize: buildGithubSearchCodeFinalizer<PartialCodeSearchQuery>(),
    }
  );
}
