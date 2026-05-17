/**
 * GitLab File Content
 *
 * Fetch file content from GitLab repositories.
 * When `ref` is omitted, GitLab's `HEAD` shorthand targets the default branch.
 *
 * @module gitlab/fileContent
 */

import type {
  GitLabAPIResponse,
  GitLabFileContentQuery,
  GitLabFileContent,
} from './types.js';
import {
  getGitlab,
  getCachedDefaultBranch,
  cacheDefaultBranch,
} from './client.js';
import { handleGitLabAPIError, createGitLabError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  parseGitLabDefaultBranch,
  parseGitLabFileContent,
} from './responseGuards.js';

/**
 * Fetch file content from GitLab.
 *
 * @param params - Query parameters
 * @param sessionId - Optional session ID for caching
 * @returns File content
 */
export async function fetchGitLabFileContentAPI(
  params: GitLabFileContentQuery,
  sessionId?: string
): Promise<GitLabAPIResponse<GitLabFileContent>> {
  // Validate required parameters
  if (!params.projectId) {
    return createGitLabError('Project ID is required', 400);
  }

  if (!params.path) {
    return createGitLabError('File path is required', 400);
  }

  const ref = params.ref || 'HEAD';

  // Generate cache key
  const cacheKey = generateCacheKey(
    'gl-api-file',
    {
      projectId: params.projectId,
      path: params.path,
      ref,
    },
    sessionId
  );

  return withDataCache<GitLabAPIResponse<GitLabFileContent>>(
    cacheKey,
    async () => fetchGitLabFileContentAPIInternal({ ...params, ref }),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchGitLabFileContentAPIInternal(
  params: GitLabFileContentQuery
): Promise<GitLabAPIResponse<GitLabFileContent>> {
  try {
    const gitlab = await getGitlab();
    const ref = params.ref || 'HEAD';

    // URL-encode the file path as required by GitLab API
    const encodedPath = encodeURIComponent(params.path);

    const file = parseGitLabFileContent(
      await gitlab.RepositoryFiles.show(params.projectId, encodedPath, ref),
      ref
    );
    if (!file) {
      return createGitLabError('Unexpected GitLab file response shape', 502);
    }

    let { content } = file;

    // Apply line filtering if requested
    if (params.startLine !== undefined || params.endLine !== undefined) {
      const lines = content.split('\n');
      const start = (params.startLine ?? 1) - 1; // Convert to 0-indexed
      const end = params.endLine ?? lines.length;
      content = lines.slice(start, end).join('\n');
    }

    return {
      data: {
        ...file,
        content,
      },
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Get the default branch for a GitLab project.
 *
 * @param projectId - Project ID
 * @returns Default branch name
 */
export async function getGitLabDefaultBranch(
  projectId: number | string
): Promise<string> {
  const cacheKey = String(projectId);
  const cached = getCachedDefaultBranch(cacheKey);
  if (cached) return cached;

  try {
    const gitlab = await getGitlab();
    const branch =
      parseGitLabDefaultBranch(await gitlab.Projects.show(projectId)) || 'main';
    cacheDefaultBranch(cacheKey, branch);
    return branch;
  } catch {
    return 'main';
  }
}

/**
 * Check if a file exists in a GitLab repository.
 *
 * @param projectId - Project ID
 * @param path - File path
 * @param ref - Branch/tag/commit reference
 * @returns True if file exists
 */
export async function gitLabFileExists(
  projectId: number | string,
  path: string,
  ref: string
): Promise<boolean> {
  try {
    const gitlab = await getGitlab();
    const encodedPath = encodeURIComponent(path);
    await gitlab.RepositoryFiles.show(projectId, encodedPath, ref);
    return true;
  } catch {
    return false;
  }
}

/**
 * Transform GitLab file content to unified format.
 */
export function transformGitLabFileContent(file: GitLabFileContent): {
  path: string;
  content: string;
  encoding: string;
  size: number;
  ref: string;
  lastCommitId: string;
} {
  return {
    path: file.file_path,
    content: file.content,
    encoding: file.encoding,
    size: file.size,
    ref: file.ref,
    lastCommitId: file.last_commit_id,
  };
}
