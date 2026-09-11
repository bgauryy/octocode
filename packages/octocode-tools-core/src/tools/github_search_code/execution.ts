import {
  GITHUB_SEARCH_TOOL_NAME,
  type GitHubSearchQuery,
} from '@octocodeai/octocode-core/schema';
import type { GitHubSearchCodeData } from '@octocodeai/octocode-core/types';
import { getOctokit } from '../../github/client.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import {
  createErrorResult,
  createSuccessResult,
  handleCatchError,
} from '../utils.js';
import {
  mapCodeSearchProviderResult,
  mapCodeSearchToolQuery,
} from '../providerMappers/codeSearch.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import type { RepoState } from './resultTypes.js';

type PartialCodeSearchQuery = WithOptionalMeta<
  Extract<GitHubSearchQuery, { operation: 'code' }>
>;

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

export async function searchGitHubCode(
  query: PartialCodeSearchQuery,
  args: ToolExecutionArgs<GitHubSearchQuery>,
  getProviderContext = createLazyProviderContext(args.authInfo)
): Promise<ProcessedBulkResult> {
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
      GITHUB_SEARCH_TOOL_NAME,
      { rawResponse: providerResult.response.rawResponseChars }
    );
  } catch (error) {
    return handleCatchError(error, query, undefined, GITHUB_SEARCH_TOOL_NAME);
  }
}
