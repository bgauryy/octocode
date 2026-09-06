import { GITHUB_SEARCH_HISTORY_TOOL_NAME } from '../../toolNames.js';
import { createSuccessResult } from '../../utils.js';
import {
  mapPullRequestProviderResultData,
  mapPullRequestToolQuery,
} from '../../providerMappers/pullRequests.js';
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
  getProviderContext: ReturnType<typeof createLazyProviderContext>,
  toolName = GITHUB_SEARCH_HISTORY_TOOL_NAME
): Promise<ProcessedBulkResult> {
  const currentProviderContext = getProviderContext();
  const effectiveQuery: PartialPRQuery = { ...parsedData };
  const contentRequest = normalizePullRequestContentRequest(
    effectiveQuery as never
  );
  const hasPrNumber = effectiveQuery.prNumber !== undefined;

  if (!hasPrNumber) {
    (effectiveQuery as { content?: unknown }).content = undefined;
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
    !hasPrNumber && Boolean((query as { content?: unknown }).content);
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
  // Only detail mode (single prNumber) emits the full per-row drill-down menu
  // (getBody/getChangedFiles/patches/…). On a LIST that ~8-fragment menu was
  // repeated on every row — pure verbosity, since the agent already has each
  // row's number and gets one runnable data-level `next` template below.
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
  resultData.pullRequests = shapedPullRequests;

  if (
    !hasPrNumber &&
    (effectiveQuery as { concise?: boolean }).concise === true
  ) {
    resultData.pullRequests = shapedPullRequests.map(pr => {
      const p = pr as { number?: unknown; title?: unknown };
      return `#${p.number} ${p.title}`;
    }) as unknown as typeof resultData.pullRequests;
  }

  const hasContent = shapedPullRequests.length > 0;

  // List mode: one runnable data-level drill-down template (using the first
  // row's number as a concrete example) replaces N per-row menus.
  if (!hasPrNumber && hasContent) {
    const firstNumber = (shapedPullRequests[0] as { number?: number }).number;
    const owner = (effectiveQuery as { owner?: string }).owner;
    const repo = (effectiveQuery as { repo?: string }).repo;
    if (firstNumber != null && owner && repo) {
      resultData.next = {
        readPr: {
          tool: 'ghGetHistoryItem',
          query: {
            operation: 'pullRequest',
            owner,
            repo,
            number: firstNumber,
            content: {
              body: true,
              changedFiles: true,
              comments: { discussion: true },
            },
          },
          why: 'Read any PR from this list — swap number for the # you want.',
          confidence: 'low',
        },
      };
    }
  }

  // Per-call result/file-change/matchString hints were computed only from
  // populated results and are dropped centrally by createSuccessResult on
  // success, so this mode no longer builds them here.
  return createSuccessResult(
    effectiveQuery,
    resultData as unknown as Record<string, unknown>,
    hasContent,
    toolName,
    {
      rawResponse: providerResult.response.rawResponseChars,
    }
  );
}
// --- end default pull-requests mode ---
