import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubSearchCodeData } from '@octocodeai/octocode-core/types';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { getOctokit } from '../../github/client.js';
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

export function hasValidCodeSearchParams(
  query: PartialCodeSearchQuery
): boolean {
  const keywords = query.keywords ?? [];
  return Boolean(
    keywords.some(keyword => keyword.trim().length > 0) ||
    query.owner ||
    query.path ||
    query.extension ||
    query.filename ||
    query.language
  );
}

function validateCodeSearchScope(
  query: PartialCodeSearchQuery
): { error: string } | undefined {
  if (query.repo && !query.owner) {
    return {
      error:
        'Repository scope requires owner. Provide both owner and repo, or omit repo for a broader search.',
    };
  }
  return undefined;
}

export type RepoState =
  | { kind: 'notFound' }
  | { kind: 'archived' }
  | { kind: 'renamed'; fullName: string };

async function probeRepoState(
  owner: string,
  repo: string,
  authInfo?: Parameters<typeof getOctokit>[0]
): Promise<RepoState | undefined> {
  try {
    const octokit = await getOctokit(authInfo);
    const { data } = await octokit.rest.repos.get({ owner, repo });
    const requested = `${owner}/${repo}`.toLowerCase();
    if (data.full_name && data.full_name.toLowerCase() !== requested) {
      return { kind: 'renamed', fullName: data.full_name };
    }
    if (data.archived) return { kind: 'archived' };
    return undefined;
  } catch (error) {
    if ((error as { status?: number }).status === 404) {
      return { kind: 'notFound' };
    }
    // Metadata probe is best-effort — never fail the search over it.
    return undefined;
  }
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
          return createErrorResult(scopeValidation.error, query);
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

        // A scoped query that matched NOTHING is ambiguous: true absence,
        // renamed repo (old name silently misses), archived repo (index may
        // lag), or a repo that never existed. One cheap metadata probe —
        // only on the scoped-zero path — lets the finalizer say which.
        if (flat.results.length === 0 && query.owner && query.repo) {
          const repoState = await probeRepoState(
            String(query.owner),
            String(query.repo),
            args.authInfo
          );
          if (repoState) {
            (flat as GitHubSearchCodeData & { repoState?: unknown }).repoState =
              repoState;
          }
        }

        return createSuccessResult(
          query,
          flat as GitHubSearchCodeData,
          flat.results.length > 0,
          TOOL_NAMES.GITHUB_SEARCH_CODE,
          {
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_SEARCH_CODE
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      finalize: buildGhSearchCodeFinalizer<PartialCodeSearchQuery>(),
    },
    args
  );
}
