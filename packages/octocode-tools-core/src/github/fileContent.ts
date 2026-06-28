import type { GitHubAPIResponse } from './githubAPI.js';
import type {
  FileContentExecutionQuery,
  GitHubFileContentApiResult,
} from '../tools/github_fetch_content/types.js';
import { getOctokit } from './client.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';

import {
  fetchRawGitHubFileContent,
  type RawContentResult,
} from './fileContentRaw.js';
import {
  applyContentPagination,
  fetchFileTimestamp,
  processFileContentAPI,
} from './fileContentProcess.js';

export async function fetchGitHubFileContentAPI(
  params: FileContentExecutionQuery,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<GitHubFileContentApiResult>> {
  const cacheKey = generateCacheKey(
    'gh-api-file-content',
    {
      owner: params.owner,
      repo: params.repo,
      path: params.path,
      branch: params.branch,
    },
    sessionId
  );

  const rawResult = await withDataCache<GitHubAPIResponse<RawContentResult>>(
    cacheKey,
    async () => {
      return await fetchRawGitHubFileContent(params, authInfo);
    },
    {
      shouldCache: (value: GitHubAPIResponse<RawContentResult>) =>
        'data' in value && !(value as { error?: unknown }).error,
      forceRefresh: params.forceRefresh === true,
    }
  );

  if (!('data' in rawResult) || !rawResult.data) {
    return rawResult as GitHubAPIResponse<GitHubFileContentApiResult>;
  }

  const branchForProcessing =
    rawResult.data.branch || rawResult.data.resolvedRef || params.branch || '';

  const processedResult = await processFileContentAPI(
    rawResult.data.rawContent,
    params.owner,
    params.repo,
    branchForProcessing,
    params.path,
    params.fullContent || false,
    params.startLine,
    params.endLine,
    params.contextLines ?? 5,
    params.matchString,
    params.matchStringIsRegex,
    params.matchStringCaseSensitive,
    params.minify ?? 'standard'
  );

  if ('error' in processedResult) {
    return {
      error: processedResult.error || 'Unknown error',
      status: 500,
      type: 'unknown' as const,
    };
  }

  const { signaturesExtracted, ...processedData } = processedResult;
  const charOffset = params.charOffset ?? 0;
  const charLength = params.charLength;
  // fullContent:true is an explicit "give me the WHOLE file in one shot" request
  // and opts out of the default char-window pagination (the documented
  // contract) — but only when no explicit window was asked for. Huge files
  // still paginate BY DEFAULT; an explicit charOffset/charLength still windows.
  const wantsWholeFile =
    params.fullContent === true && charOffset === 0 && charLength === undefined;
  const paginatedResult =
    signaturesExtracted || wantsWholeFile
      ? processedData
      : applyContentPagination(processedData, charOffset, charLength);

  const isContinuationPage = (params.charOffset ?? 0) > 0;
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
            branch: params.branch,
            ts: true,
          },
          sessionId
        ),
        () =>
          fetchFileTimestamp(
            octokit,
            params.owner,
            params.repo,
            params.path,
            params.branch
          ),
        {
          shouldCache: value => value !== null,
          forceRefresh: params.forceRefresh === true,
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
