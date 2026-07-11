import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { createSuccessResult, createErrorResult } from '../../utils.js';
import {
  mapPullRequestProviderResultData,
  mapPullRequestToolQuery,
} from '../../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../../providerExecution.js';
import { normalizePullRequestContentRequest } from '../contentRequest.js';
import { shapePullRequestForContent } from '../contentResponse.js';
import type { ProcessedBulkResult } from '../../../types/toolResults.js';
import type {
  GitHubPullRequestSearchInput,
  GitHubPullRequestSearchQuery,
  PartialPRQuery,
} from './types.js';

// --- default mode: full-text/filter search over pull requests ---
export async function handlePullRequestsMode(
  query: GitHubPullRequestSearchInput,
  parsedData: GitHubPullRequestSearchQuery | undefined,
  getProviderContext: ReturnType<typeof createLazyProviderContext>
): Promise<ProcessedBulkResult> {
  const currentProviderContext = getProviderContext();
  const effectiveQuery: PartialPRQuery = { ...parsedData };
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
    (effectiveQuery.prNumber && effectiveQuery.owner && effectiveQuery.repo);

  if (!hasValidParams) {
    return createErrorResult(
      'At least one valid search parameter, filter, or PR number is required.',
      query
    );
  }

  const providerResult = await executeProviderOperation(effectiveQuery, () =>
    currentProviderContext.provider.searchPullRequests(
      mapPullRequestToolQuery(effectiveQuery)
    )
  );

  if (providerResult.ok === false) {
    return providerResult.result;
  }

  const includeFileChanges = hasPrNumber
    ? contentRequest.changedFiles || contentRequest.patches.mode !== 'none'
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
  // Always emit per-row drill-down hints (getBody/getChangedFiles/etc, keyed
  // off that row's own PR number via baseQuery) — list mode used to dead-end
  // with no next-step guidance because this was gated to detail-fetch only,
  // even though nextCalls() already targets the correct row regardless of
  // which mode produced it.
  const showContentMap = true;
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
}
// --- end default pull-requests mode ---
