import type { CallToolResult } from '@modelcontextprotocol/server';
import type { z } from 'zod';

import { fetchCommit } from '../../github/commit.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { createLazyProviderContext } from '../providerExecution.js';
import {
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
} from '../toolNames.js';
import {
  createErrorResult,
  createSuccessResult,
  handleCatchError,
  safeParseOrError,
} from '../utils.js';
import { handleCommitsMode } from './execution/commitsMode.js';
import { handleIssuesMode } from './execution/issuesMode.js';
import { handlePullRequestsMode } from './execution/pullRequestsMode.js';
import type {
  GitHubPullRequestSearchInput,
  GitHubPullRequestSearchQuery,
} from './execution/types.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import {
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
} from './historySchemes.js';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';
import { withDiffContinuations } from './historyDiffContinuations.js';
import { withContentContinuations } from './historyPartialContinuations.js';
import {
  withSearchPageContinuation,
  type SearchHistoryOperation,
} from './historySearchPagination.js';

type PublicArgs = ToolExecutionArgs<Record<string, unknown>>;
type SearchHistoryQuery = z.infer<typeof GitHubSearchHistoryQueryLocalSchema>;
type HistoryItemQuery = z.infer<typeof GitHubGetHistoryItemQueryLocalSchema>;

function parseInternalQuery(query: Record<string, unknown>) {
  return safeParseOrError(GitHubPullRequestSearchQueryLocalSchema, query);
}

function withoutOperation(query: Record<string, unknown>) {
  const { operation: _operation, ...rest } = query;
  return rest;
}

async function executeSearchQuery(
  query: Record<string, unknown>,
  args: PublicArgs,
  getProviderContext: ReturnType<typeof createLazyProviderContext>
): Promise<ProcessedBulkResult> {
  const parsedPublic = safeParseOrError(
    GitHubSearchHistoryQueryLocalSchema,
    query
  );
  if (parsedPublic.ok === false) return parsedPublic.error;
  const publicQuery = parsedPublic.data as SearchHistoryQuery;
  const operation = publicQuery.operation as SearchHistoryOperation;
  const internalQuery = {
    ...withoutOperation(publicQuery),
    type:
      operation === 'pullRequests'
        ? 'prs'
        : operation === 'issues'
          ? 'issues'
          : 'commits',
  } as GitHubPullRequestSearchInput;
  const parsedInternal = parseInternalQuery(internalQuery);
  if (parsedInternal.ok === false) return parsedInternal.error;

  if (operation === 'pullRequests') {
    return withSearchPageContinuation(
      await handlePullRequestsMode(
        internalQuery,
        parsedInternal.data,
        getProviderContext,
        GITHUB_SEARCH_HISTORY_TOOL_NAME
      ),
      query,
      operation
    );
  }
  if (operation === 'issues') {
    return withSearchPageContinuation(
      await handleIssuesMode(
        internalQuery,
        parsedInternal.data,
        args.authInfo,
        GITHUB_SEARCH_HISTORY_TOOL_NAME
      ),
      query,
      operation
    );
  }
  return withSearchPageContinuation(
    await handleCommitsMode(
      internalQuery,
      parsedInternal.data,
      args.authInfo,
      GITHUB_SEARCH_HISTORY_TOOL_NAME
    ),
    query,
    operation
  );
}

async function executeItemQuery(
  query: Record<string, unknown>,
  args: PublicArgs,
  getProviderContext: ReturnType<typeof createLazyProviderContext>
): Promise<ProcessedBulkResult> {
  const parsedPublic = safeParseOrError(
    GitHubGetHistoryItemQueryLocalSchema,
    query
  );
  if (parsedPublic.ok === false) return parsedPublic.error;
  const item = parsedPublic.data as HistoryItemQuery;

  if (item.operation === 'commit') {
    const result = await fetchCommit(
      {
        owner: String(item.owner),
        repo: String(item.repo),
        ref: String(item.ref),
        ...(typeof item.fileBatch === 'number'
          ? { fileBatch: item.fileBatch }
          : {}),
        ...(typeof item.includeDiff !== 'boolean'
          ? {}
          : { includeDiff: item.includeDiff }),
        ...(typeof item.path !== 'string' ? {} : { path: item.path }),
        ...(typeof item.filePage !== 'number'
          ? {}
          : { filePage: item.filePage }),
        ...(typeof item.pageSize !== 'number'
          ? {}
          : { itemsPerPage: item.pageSize }),
        ...(typeof item.charOffset !== 'number'
          ? {}
          : { charOffset: item.charOffset }),
        ...(typeof item.charLength !== 'number'
          ? {}
          : { charLength: item.charLength }),
      },
      args.authInfo
    );
    if (isGitHubAPIError(result)) {
      return createErrorResult(result, query, {
        toolName: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
      });
    }
    return createSuccessResult(
      query,
      withDiffContinuations(
        result.data as unknown as Record<string, unknown>,
        query
      ),
      true,
      GITHUB_GET_HISTORY_ITEM_TOOL_NAME
    );
  }

  const mapped = withoutOperation(item as Record<string, unknown>);
  const internalQuery = {
    ...mapped,
    ...(item.operation === 'pullRequest'
      ? { type: 'prs', prNumber: item.number, number: undefined }
      : item.operation === 'issue'
        ? { type: 'issues', issueNumber: item.number, number: undefined }
        : { type: 'commits' }),
  } as GitHubPullRequestSearchInput;
  delete (internalQuery as Record<string, unknown>).number;
  const parsedInternal = parseInternalQuery(internalQuery);
  if (parsedInternal.ok === false) return parsedInternal.error;

  if (item.operation === 'pullRequest') {
    return withContentContinuations(
      await handlePullRequestsMode(
        internalQuery,
        parsedInternal.data,
        getProviderContext,
        GITHUB_GET_HISTORY_ITEM_TOOL_NAME
      ),
      item.operation
    );
  }
  if (item.operation === 'issue') {
    return withContentContinuations(
      await handleIssuesMode(
        internalQuery,
        parsedInternal.data,
        args.authInfo,
        GITHUB_GET_HISTORY_ITEM_TOOL_NAME
      ),
      item.operation
    );
  }
  return handleCommitsMode(
    internalQuery,
    parsedInternal.data as GitHubPullRequestSearchQuery,
    args.authInfo,
    GITHUB_GET_HISTORY_ITEM_TOOL_NAME
  );
}

function executeHistoryTool(
  args: PublicArgs,
  toolName: string,
  executeQuery: (
    query: Record<string, unknown>,
    args: PublicArgs,
    getProviderContext: ReturnType<typeof createLazyProviderContext>
  ) => Promise<ProcessedBulkResult>
): Promise<CallToolResult> {
  const getProviderContext = createLazyProviderContext(args.authInfo);
  return executeBulkOperation(
    args.queries,
    async query => {
      try {
        return await executeQuery(query, args, getProviderContext);
      } catch (error) {
        return handleCatchError(error, query, undefined, toolName);
      }
    },
    { toolName },
    args
  );
}

export function searchMultipleGitHubHistory(
  args: PublicArgs
): Promise<CallToolResult> {
  return executeHistoryTool(
    args,
    GITHUB_SEARCH_HISTORY_TOOL_NAME,
    executeSearchQuery
  );
}

export function getMultipleGitHubHistoryItems(
  args: PublicArgs
): Promise<CallToolResult> {
  return executeHistoryTool(
    args,
    GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
    executeItemQuery
  );
}
