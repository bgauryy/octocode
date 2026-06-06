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
    params.matchStringContextLines ?? 5,
    params.matchString
  );

  if ('error' in processedResult) {
    return {
      error: processedResult.error || 'Unknown error',
      status: 500,
      type: 'unknown' as const,
    };
  }

  if (!params.noTimestamp) {
    try {
      const octokit = await getOctokit(authInfo);
      const timestampInfo = await fetchFileTimestamp(
        octokit,
        params.owner,
        params.repo,
        params.path,
        params.branch
      );
      if (timestampInfo) {
        processedResult.lastModified = timestampInfo.lastModified;
        processedResult.lastModifiedBy = timestampInfo.lastModifiedBy;
      }
    } catch {
      void 0;
    }
  }

  return {
    data: processedResult,
    status: 200,
    rawResponseChars: rawResult.rawResponseChars,
  };
}
