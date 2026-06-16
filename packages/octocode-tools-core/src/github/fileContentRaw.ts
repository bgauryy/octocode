import { RequestError } from 'octokit';
import type { GetContentParameters, GitHubAPIResponse } from './githubAPI.js';
import type { z } from 'zod';
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

async function decodeBase64Content(
  base64: string,
  filePath: string
): Promise<GitHubAPIResponse<string>> {
  const stripped = base64.replace(/\s/g, '');
  if (!stripped) {
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
    const buffer = Buffer.from(stripped, 'base64');
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
  void filePath;
}

async function fetchContentViaBlob(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  sha: string,
  filePath: string
): Promise<GitHubAPIResponse<string>> {
  try {
    const blobResult = await octokit.rest.git.getBlob({
      owner,
      repo,
      file_sha: sha,
    });
    const { content, encoding } = blobResult.data;
    if (encoding === 'base64') {
      return decodeBase64Content(content, filePath);
    }
    if (encoding === 'utf-8') {
      return { data: content, status: 200 };
    }
    return {
      error: `Unsupported blob encoding: ${encoding}`,
      type: 'unknown' as const,
      status: 415,
    };
  } catch (err: unknown) {
    return handleGitHubAPIError(err);
  }
}

async function fetchContentViaTreeFallback(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  filePath: string,
  branch?: string | null,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RawContentResult>> {
  try {
    const parentPath = filePath.split('/').slice(0, -1).join('/');
    const fileName = filePath.split('/').pop();
    if (!fileName) {
      return {
        error: `Cannot determine file name from path: ${filePath}`,
        type: 'unknown' as const,
        status: 400,
      };
    }

    const ref =
      branch ||
      (await resolveDefaultBranch(owner, repo, authInfo).catch(() => 'HEAD'));

    const dirResult = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: parentPath || '',
      ref,
    });

    if (!Array.isArray(dirResult.data)) {
      return {
        error: `Expected directory listing for ${parentPath || 'root'}`,
        type: 'unknown' as const,
        status: 500,
      };
    }

    const entry = (
      dirResult.data as Array<{ name: string; sha: string; type: string }>
    ).find(e => e.name === fileName && e.type === 'file');

    if (!entry) {
      return {
        error: `File ${fileName} not found in ${parentPath || 'root'}`,
        type: 'unknown' as const,
        status: 404,
      };
    }

    const decoded = await fetchContentViaBlob(
      octokit,
      owner,
      repo,
      entry.sha,
      filePath
    );
    if ('error' in decoded)
      return decoded as GitHubAPIResponse<RawContentResult>;

    return {
      data: {
        rawContent: decoded.data,
        branch: typeof ref === 'string' ? ref : undefined,
        resolvedRef: typeof ref === 'string' ? ref : 'HEAD',
      },
      status: 200,
    };
  } catch (err: unknown) {
    return handleGitHubAPIError(err);
  }
}

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
      } else if (error instanceof RequestError && error.status === 413) {
        return await fetchContentViaTreeFallback(
          octokit,
          owner,
          repo,
          filePath,
          branch || actualBranch,
          authInfo
        );
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
      const contentStr = typeof data.content === 'string' ? data.content : '';
      const fileSize = (data as { size?: number }).size ?? 0;

      let decoded: GitHubAPIResponse<string>;
      if (contentStr.length > 0) {
        decoded = await decodeBase64Content(contentStr, filePath);
      } else if (
        fileSize > 0 &&
        'sha' in data &&
        typeof data.sha === 'string' &&
        data.sha
      ) {
        decoded = await fetchContentViaBlob(
          octokit,
          owner,
          repo,
          data.sha,
          filePath
        );
      } else {
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
      'GitHub Contents API paths are case-sensitive. Verify exact file casing with ghViewRepoStructure.'
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

    if (suggestions.length === 0 && nameNoExt.length >= 3) {
      const prefixMatches = files.filter(f => {
        const fBase = f.name.replace(/\.[^/.]+$/, '');
        return (
          fBase !== nameNoExt &&
          fBase.length >= 3 &&
          (nameNoExt.startsWith(fBase) || fBase.startsWith(nameNoExt))
        );
      });
      prefixMatches.forEach(f => suggestions.push(f.path));
    }

    return Array.from(new Set(suggestions)).slice(0, 3);
  } catch {
    return [];
  }
}
