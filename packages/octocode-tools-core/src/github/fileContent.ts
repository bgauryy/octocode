import { matchContext } from '../utils/file/matchContext.js';
import type { GitHubAPIResponse } from './githubAPI.js';
import type {
  FileContentExecutionQuery,
  GitHubFileContentApiResult,
} from '../tools/github_fetch_content/types.js';
import { getOctokit } from './client.js';
import { generateCacheKey } from '../utils/http/cache/key.js';
import { withDataCache } from '../utils/http/cache/dataCache.js';
import { AuthInfo } from '@modelcontextprotocol/server';
import { fetchCachedRawGitHubFileContent } from './fileContentRaw/cache.js';
import {
  applyContentPagination,
  fetchFileTimestamp,
} from './fileContentPagination.js';
import { processFileContentAPI } from './fileContentProcess.js';

export async function fetchGitHubFileContentAPI(
  params: FileContentExecutionQuery,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<GitHubFileContentApiResult>> {
  const { rawResult, auth } = await fetchCachedRawGitHubFileContent(
    params,
    authInfo,
    sessionId
  );

  if (!('data' in rawResult) || !rawResult.data) {
    return rawResult as GitHubAPIResponse<GitHubFileContentApiResult>;
  }

  const branchForProcessing =
    rawResult.data.branch || rawResult.data.resolvedRef || params.branch || '';

  const context = matchContext(params);
  const processedResult = await processFileContentAPI(
    rawResult.data.rawContent,
    params.owner,
    params.repo,
    branchForProcessing,
    params.path,
    params.fullContent || false,
    params.startLine,
    params.endLine,
    context.contextLines ?? 0,
    params.matchString,
    params.matchStringIsRegex,
    params.matchStringCaseSensitive,
    params.minify ?? 'none',
    context.contextBytes
  );

  if ('error' in processedResult) {
    return {
      error: processedResult.error || 'Unknown error',
      status: 500,
      type: 'unknown' as const,
    };
  }

  // A scanner limit is a terminal selected-view diagnostic, not an empty
  // successful file to paginate or enrich with a timestamp request.
  if (processedResult.terminalLimit) {
    return {
      data: processedResult,
      status: 200,
      rawResponseChars: rawResult.rawResponseChars,
    };
  }

  const { noTimestamp: _noTimestamp, ...paginationQuery } = params;
  const paginatedResult = await applyContentPagination(processedResult, {
    ...paginationQuery,
    branch: branchForProcessing,
  });
  const isContinuationPage = (params.offset ?? 0) > 0;
  if (!params.noTimestamp && !isContinuationPage) {
    try {
      const octokit = await getOctokit(authInfo);
      const timestampInfo = await withDataCache(
        generateCacheKey(
          'gh-api-file-content',
          {
            owner: params.owner,
            repo: params.repo,
            path: params.path,
            branch: branchForProcessing,
            ts: true,
            auth,
          },
          sessionId
        ),
        () =>
          fetchFileTimestamp(
            octokit,
            params.owner,
            params.repo,
            params.path,
            branchForProcessing
          ),
        {
          shouldCache: value => value !== null,
          forceRefresh: params.forceRefresh === true,
          cacheRole: 'helper',
        }
      );
      if (timestampInfo) {
        paginatedResult.lastModified = timestampInfo.lastModified;
        paginatedResult.lastModifiedBy = timestampInfo.lastModifiedBy;
      }
    } catch {
      void 0;
    }
  }

  return {
    data: paginatedResult,
    status: 200,
    rawResponseChars: rawResult.rawResponseChars,
  };
}
