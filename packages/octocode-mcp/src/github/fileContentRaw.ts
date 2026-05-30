import { RequestError } from 'octokit';
import type { GetContentParameters, GitHubAPIResponse } from './githubAPI.js';
import type { z } from 'zod/v4';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { GitHubApiFileItem } from '../tools/github_view_repo_structure/types.js';
import {
  getOctokit,
  OctokitWithThrottling,
  resolveDefaultBranch,
} from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { FILE_OPERATION_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import { countSerializedChars } from '../utils/response/charSavings.js';

export interface RawContentResult {
  rawContent: string;
  branch?: string;
  resolvedRef: string;
}

async function handle404WithBranch(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  error: RequestError,
  contentParams: GetContentParameters,
  owner: string,
  repo: string,
  filePath: string,
  branch: string,
  authInfo?: AuthInfo
): Promise<
  | {
      result: Awaited<ReturnType<typeof octokit.rest.repos.getContent>>;
      actualBranch: string;
    }
  | GitHubAPIResponse<RawContentResult>
> {
  const defaultBranch = await resolveDefaultBranch(owner, repo, authInfo);
  const isCommonDefaultGuess = branch === 'main' || branch === 'master';

  if (isCommonDefaultGuess && branch !== defaultBranch) {
    try {
      const result = await octokit.rest.repos.getContent({
        ...contentParams,
        ref: defaultBranch,
      });
      return { result, actualBranch: defaultBranch };
    } catch {
      throw error;
    }
  }

  const apiError = handleGitHubAPIError(error);
  const suggestion =
    branch === defaultBranch
      ? undefined
      : `Branch '${branch}' not found. Default branch is '${defaultBranch}'. Ask user: Do you want to get the file from '${defaultBranch}' instead?`;

  const pathSuggestions = await findPathSuggestions(
    octokit,
    owner,
    repo,
    filePath,
    branch || defaultBranch
  );
  if (pathSuggestions.length > 0) {
    apiError.hints = [
      ...(apiError.hints || []),
      ...buildPathSuggestionHints(filePath, pathSuggestions),
    ];
  }
  return { ...apiError, ...(suggestion && { scopesSuggestion: suggestion }) };
}

async function handle404NoBranch(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  error: RequestError,
  owner: string,
  repo: string,
  filePath: string,
  branch?: string
): Promise<GitHubAPIResponse<RawContentResult>> {
  const apiError = handleGitHubAPIError(error);
  const pathSuggestions = await findPathSuggestions(
    octokit,
    owner,
    repo,
    filePath,
    branch || 'main'
  );
  if (pathSuggestions.length > 0) {
    apiError.hints = [
      ...(apiError.hints || []),
      ...buildPathSuggestionHints(filePath, pathSuggestions),
    ];
  }
  return apiError;
}

async function decodeFileContent(data: {
  content?: string;
  size?: number;
  type: string;
}): Promise<GitHubAPIResponse<string>> {
  const fileSize = data.size || 0;
  const MAX_FILE_SIZE = 300 * 1024;

  if (fileSize > MAX_FILE_SIZE) {
    await logSessionError(
      TOOL_NAMES.GITHUB_FETCH_CONTENT,
      FILE_OPERATION_ERRORS.FILE_TOO_LARGE.code
    );
    return {
      error: FILE_OPERATION_ERRORS.FILE_TOO_LARGE.message(
        Math.round(fileSize / 1024),
        Math.round(MAX_FILE_SIZE / 1024),
        TOOL_NAMES.GITHUB_SEARCH_CODE
      ),
      type: 'unknown' as const,
      status: 413,
    };
  }

  const base64Content = (data.content || '').replace(/\s/g, '');
  if (!data.content || !base64Content) {
    await logSessionError(
      TOOL_NAMES.GITHUB_FETCH_CONTENT,
      FILE_OPERATION_ERRORS.FILE_EMPTY.code
    );
    return {
      error: FILE_OPERATION_ERRORS.FILE_EMPTY.message,
      type: 'unknown' as const,
      status: 404,
    };
  }

  try {
    const buffer = Buffer.from(base64Content, 'base64');
    if (buffer.indexOf(0) !== -1) {
      await logSessionError(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        FILE_OPERATION_ERRORS.BINARY_FILE.code
      );
      return {
        error: FILE_OPERATION_ERRORS.BINARY_FILE.message,
        type: 'unknown' as const,
        status: 415,
      };
    }
    return { data: buffer.toString('utf-8'), status: 200 };
  } catch {
    await logSessionError(
      TOOL_NAMES.GITHUB_FETCH_CONTENT,
      FILE_OPERATION_ERRORS.DECODE_FAILED.code
    );
    return {
      error: FILE_OPERATION_ERRORS.DECODE_FAILED.message,
      type: 'unknown' as const,
      status: 422,
    };
  }
}

/** Raw GitHub file fetch for caching; line/match slicing happens after cache. */
export async function fetchRawGitHubFileContent(
  params: FileContentQuery,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RawContentResult>> {
  try {
    const octokit = await getOctokit(authInfo);
    const { owner, repo, path: filePath, branch } = params;

    const contentParams: GetContentParameters = {
      owner,
      repo,
      path: filePath,
      ...(branch && { ref: branch }),
    };

    let result;
    let actualBranch = branch;
    try {
      result = await octokit.rest.repos.getContent(contentParams);
    } catch (error: unknown) {
      if (error instanceof RequestError && error.status === 404) {
        if (branch) {
          const fallback = await handle404WithBranch(
            octokit,
            error,
            contentParams,
            owner,
            repo,
            filePath,
            branch,
            authInfo
          );
          if ('result' in fallback) {
            result = fallback.result;
            actualBranch = fallback.actualBranch;
          } else {
            return fallback;
          }
        } else {
          return await handle404NoBranch(
            octokit,
            error,
            owner,
            repo,
            filePath,
            branch
          );
        }
      } else {
        throw error;
      }
    }

    const data = result.data;

    if (Array.isArray(data)) {
      await logSessionError(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        FILE_OPERATION_ERRORS.PATH_IS_DIRECTORY.code
      );
      return {
        error: FILE_OPERATION_ERRORS.PATH_IS_DIRECTORY.message(
          TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
        ),
        type: 'unknown' as const,
        status: 400,
      };
    }

    if ('content' in data && data.type === 'file') {
      const decoded = await decodeFileContent(data);
      if ('error' in decoded)
        return decoded as GitHubAPIResponse<RawContentResult>;

      if (!actualBranch && !branch) {
        try {
          actualBranch = await resolveDefaultBranch(owner, repo, authInfo);
        } catch {
          void 0;
        }
      }

      return {
        data: {
          rawContent: decoded.data,
          branch: actualBranch || undefined,
          resolvedRef: actualBranch || branch || 'HEAD',
        },
        status: 200,
        rawResponseChars: countSerializedChars(data),
      };
    }

    await logSessionError(
      TOOL_NAMES.GITHUB_FETCH_CONTENT,
      FILE_OPERATION_ERRORS.UNSUPPORTED_TYPE.code
    );
    return {
      error: FILE_OPERATION_ERRORS.UNSUPPORTED_TYPE.message(data.type),
      type: 'unknown' as const,
      status: 415,
    };
  } catch (error: unknown) {
    return handleGitHubAPIError(error);
  }
}

function buildPathSuggestionHints(
  requestedPath: string,
  suggestions: string[]
): string[] {
  const targetName = requestedPath.split('/').pop() || '';
  const isCaseMismatch = suggestions.some(s => {
    const suggestedName = s.split('/').pop() || '';
    return (
      suggestedName.toLowerCase() === targetName.toLowerCase() &&
      suggestedName !== targetName
    );
  });

  const hints: string[] = [];
  if (isCaseMismatch) {
    hints.push(
      'GitHub Contents API paths are case-sensitive. Verify exact file casing with githubViewRepoStructure.'
    );
  }
  hints.push(`Did you mean: ${suggestions.join(', ')}?`);
  return hints;
}

async function findPathSuggestions(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  filePath: string,
  branch: string
): Promise<string[]> {
  try {
    const parentPath = filePath.split('/').slice(0, -1).join('/');
    const targetName = filePath.split('/').pop();

    if (!targetName) return [];

    const parentContent = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: parentPath,
      ref: branch,
    });

    if (!Array.isArray(parentContent.data)) return [];

    const files = parentContent.data as GitHubApiFileItem[];
    const suggestions: string[] = [];

    const caseMatch = files.find(
      f => f.name.toLowerCase() === targetName.toLowerCase()
    );
    if (caseMatch) suggestions.push(caseMatch.path);

    const nameNoExt = targetName.replace(/\.[^/.]+$/, '');
    const extMatches = files.filter(f => {
      if (f.name === targetName) return false;
      if (f.name.startsWith(nameNoExt + '.')) return true;
      return false;
    });

    extMatches.forEach(f => suggestions.push(f.path));

    return Array.from(new Set(suggestions)).slice(0, 3);
  } catch {
    return [];
  }
}
