import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  FileContentQuery,
  FileContentResult,
} from '../types.js';

import { fetchGitHubFileContentAPI } from '../../github/fileContent.js';

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
    sourceChars: data.sourceChars,
    sourceBytes: data.sourceBytes,
    contentView: data.contentView,
    ref: data.branch || query.ref || '',
    lastModified: data.lastModified,
    lastModifiedBy: data.lastModifiedBy,
    pagination: data.pagination,
    isPartial: data.isPartial,
    startLine: data.startLine,
    endLine: data.endLine,
    matchRanges: data.matchRanges,
    warnings: buildContentWarnings(data, query),
    matchNotFound: data.matchNotFound,
    searchedFor: data.searchedFor,
  };
}

function buildContentWarnings(
  data: GitHubFileContentApiData,
  query: FileContentQuery
): string[] | undefined {
  if (data.matchNotFound === true) {
    const result = data as { hints?: string[] };
    if (Array.isArray(result.hints) && result.hints.length > 0) {
      const scanned =
        typeof data.totalLines === 'number'
          ? ` (${data.totalLines} lines scanned)`
          : '';
      return result.hints.map((h: string) =>
        h.replace(' in file', ` in file${scanned}`)
      );
    }
    const anchor = data.searchedFor ?? query.matchString ?? '';
    const scanned =
      typeof data.totalLines === 'number'
        ? ` (${data.totalLines} lines scanned)`
        : '';
    const regexAlreadyTried = query.matchStringIsRegex === true;
    const suggestion = regexAlreadyTried
      ? 'Try a different pattern, widen the anchor, or use fullContent=true to inspect the file.'
      : 'Try matchStringIsRegex=true for pattern matching, a different anchor, or fullContent=true.';
    return [`No matches for "${anchor}" in file${scanned}. ${suggestion}`];
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
    type: 'file' as const,
    branch: query.ref,
    startLine: query.startLine,
    endLine: query.endLine,
    matchString: query.matchString,
    contextLines: query.contextLines ?? 5,
    matchStringIsRegex: query.matchStringIsRegex,
    matchStringCaseSensitive: query.matchStringCaseSensitive,
    charOffset: query.charOffset,
    charLength: query.charLength,
    fullContent: query.fullContent,
    forceRefresh: query.forceRefresh,
    minify: query.minify ?? 'standard',
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };

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

  const processedHints = (result.data as { hints?: string[] }).hints;
  return {
    data: transformFileContentResult(result.data, query),
    status: 200,
    provider: 'github',
    rawResponseChars:
      result.rawResponseChars ?? countSerializedChars(result.data),
    ...(processedHints?.length ? { hints: processedHints } : {}),
  };
}
