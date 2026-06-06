import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  FileContentQuery,
  FileContentResult,
} from '../types.js';

import { fetchGitHubFileContentAPI } from '../../github/fileContent.js';

import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type GHFileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { GitHubFileContentApiData } from '../../tools/github_fetch_content/types.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';
export { parseGitHubProjectId } from './utils.js';

export function transformFileContentResult(
  data: GitHubFileContentApiData,
  query: FileContentQuery
): FileContentResult {
  return {
    path: data.path || query.path,
    content: data.content || '',
    encoding: 'utf-8',
    size: data.content?.length || 0,
    totalLines: data.totalLines,
    ref: data.branch || query.ref || '',
    lastModified: data.lastModified,
    lastModifiedBy: data.lastModifiedBy,
    pagination: data.pagination,
    isPartial: data.isPartial,
    startLine: data.startLine,
    endLine: data.endLine,
    warnings: buildContentWarnings(data, query),
  };
}

function buildContentWarnings(
  data: GitHubFileContentApiData,
  query: FileContentQuery
): string[] | undefined {
  if (data.matchNotFound === true) {
    const anchor = data.searchedFor ?? query.matchString ?? '';
    const scanned =
      typeof data.totalLines === 'number'
        ? ` (${data.totalLines} lines scanned)`
        : '';
    return [
      `No matches for "${anchor}" in file${scanned}. Try matchStringIsRegex=true, a different anchor, or fullContent=true.`,
    ];
  }
  return data.warnings ?? data.matchLocations;
}

export async function getFileContent(
  query: FileContentQuery,
  authInfo?: AuthInfo,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): Promise<ProviderResponse<FileContentResult>> {
  const { owner, repo } = parseProjectId(query.projectId);

  if (!owner || !repo) {
    return {
      error: 'Project ID is required for file content',
      status: 400,
      provider: 'github',
    };
  }

  const githubQuery = {
    owner,
    repo,
    path: query.path,
    branch: query.ref,
    startLine: query.startLine,
    endLine: query.endLine,
    matchString: query.matchString,
    matchStringContextLines: query.matchStringContextLines,
    charOffset: query.charOffset,
    charLength: query.charLength,
    fullContent: query.fullContent,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GHFileContentQuery;

  const result = await fetchGitHubFileContentAPI(githubQuery, authInfo);

  if (isGitHubAPIError(result)) {
    return createGitHubProviderError(result);
  }

  if (!result.data) {
    return {
      error: 'No data returned from GitHub API',
      status: 500,
      provider: 'github',
    };
  }

  return {
    data: transformFileContentResult(result.data, query),
    status: 200,
    provider: 'github',
    rawResponseChars:
      result.rawResponseChars ?? countSerializedChars(result.data),
  };
}
