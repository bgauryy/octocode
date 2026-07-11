import { RequestError } from 'octokit';
import type { Octokit } from 'octokit';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import type {
  GitHubApiFileItem,
  GitHubRepositoryStructureError,
} from '../../tools/github_view_repo_structure/types.js';
import { handleGitHubAPIError } from '../errors.js';
import { REPOSITORY_ERRORS } from '../../errors/domainErrors.js';
import { resolveDefaultBranch } from '../client.js';
import { extractEtag } from '../responseHeaders.js';

export interface ContentResolution {
  data: unknown;
  workingBranch: string;
  repoDefaultBranch?: string;
  etag?: string;
  notModified?: boolean;
}

export async function resolveContentWithBranchFallback(
  octokit: Octokit,
  owner: string,
  repo: string,
  cleanPath: string,
  branch: string | undefined,
  authInfo?: AuthInfo,
  ifNoneMatch?: string
): Promise<ContentResolution | GitHubRepositoryStructureError> {
  let workingBranch: string;
  // Capture the resolved default branch so callers get a `defaultBranch` hint.
  // Only known when we resolve it (no explicit branch given); when the caller
  // pinned a branch the repo default is unknown without an extra API call, so
  // the field stays absent rather than being fabricated.
  let repoDefaultBranch: string | undefined;
  try {
    if (branch) {
      workingBranch = branch;
    } else {
      repoDefaultBranch = await resolveDefaultBranch(owner, repo, authInfo);
      workingBranch = repoDefaultBranch;
    }
  } catch (repoError) {
    const apiError = handleGitHubAPIError(repoError);
    return {
      error: REPOSITORY_ERRORS.NOT_FOUND.message(owner, repo, apiError.error),
      status: apiError.status,
    };
  }

  try {
    const result = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: cleanPath || '',
      ref: workingBranch,
      ...(ifNoneMatch ? { headers: { 'If-None-Match': ifNoneMatch } } : {}),
    });
    const etag = extractEtag(result.headers);
    return {
      data: result.data,
      workingBranch,
      ...(repoDefaultBranch !== undefined ? { repoDefaultBranch } : {}),
      ...(etag ? { etag } : {}),
    };
  } catch (error: unknown) {
    if (error instanceof RequestError && error.status === 304) {
      return {
        data: null,
        workingBranch,
        ...(repoDefaultBranch !== undefined ? { repoDefaultBranch } : {}),
        etag: ifNoneMatch,
        notModified: true,
      };
    }
    if (!(error instanceof RequestError && error.status === 404)) {
      const apiError = handleGitHubAPIError(error);
      return {
        error: REPOSITORY_ERRORS.ACCESS_FAILED.message(
          owner,
          repo,
          apiError.error
        ),
        status: apiError.status,
        rateLimitRemaining: apiError.rateLimitRemaining,
        rateLimitReset: apiError.rateLimitReset,
        retryAfter: apiError.retryAfter,
      };
    }

    const apiError = handleGitHubAPIError(error);
    return {
      error: REPOSITORY_ERRORS.PATH_NOT_FOUND.message(
        cleanPath,
        owner,
        repo,
        workingBranch
      ),
      status: apiError.status,
    };
  }
}

export function mapApiItems(items: unknown[]): GitHubApiFileItem[] {
  return items.map(raw => {
    const item = raw as GitHubApiFileItem;
    return {
      name: item.name,
      path: item.path,
      type: item.type as 'file' | 'dir',
      size: 'size' in item ? item.size : undefined,
      download_url: 'download_url' in item ? item.download_url : undefined,
      url: item.url,
      html_url: item.html_url,
      git_url: item.git_url,
      sha: item.sha,
    } as GitHubApiFileItem;
  });
}
