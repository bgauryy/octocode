/**
 * PR content fetching — comments, commits, file changes, and item transformation.
 * Extracted from pullRequestSearch.ts.
 */
import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PRCommentItem,
  CommitInfo,
  DiffEntry,
  CommitFileInfo,
  IssueSearchResultItem,
  PullRequestSimple,
  PullRequestItem,
  IssueComment,
} from './githubAPI';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { logSessionError } from '../session.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import { getOctokit, OctokitWithThrottling } from './client';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import {
  createBasePRTransformation,
  normalizeOwnerRepo,
  applyPartialContentFilter,
} from './prTransformation.js';

async function fetchPRComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number
): Promise<PRCommentItem[]> {
  try {
    const commentsResult = await octokit.rest.issues.listComments({
      owner,
      repo,
      issue_number: prNumber,
    });

    return commentsResult.data.map(
      (comment: IssueComment): PRCommentItem => ({
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(comment.body ?? '').content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
      })
    );
  } catch {
    return [];
  }
}

export async function transformPullRequestItemFromSearch(
  item: IssueSearchResultItem,
  params: GitHubPullRequestsSearchParams,
  octokit: InstanceType<typeof OctokitWithThrottling>
): Promise<GitHubPullRequestItem> {
  // Cast to RawPRData - Search API items may have merged_at in extended response
  const rawItem = item as IssueSearchResultItem & { merged_at?: string | null };
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(rawItem);

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  const type = params.type || 'metadata';
  const shouldFetchContent =
    type === 'fullContent' || type === 'partialContent' || type === 'metadata';

  if (shouldFetchContent || item.pull_request) {
    try {
      const { owner, repo } = normalizeOwnerRepo(params);

      if (owner && repo) {
        const prDetails = await octokit.rest.pulls.get({
          owner,
          repo,
          pull_number: item.number,
        });

        if (prDetails.data) {
          result.head = prDetails.data.head?.ref;
          result.head_sha = prDetails.data.head?.sha;
          result.base = prDetails.data.base?.ref;
          result.base_sha = prDetails.data.base?.sha;
          result.draft = prDetails.data.draft ?? false;

          if (prDetails.data.merged_at) {
            result.merged_at = prDetails.data.merged_at;
          }

          if (shouldFetchContent) {
            const fileChanges = await fetchPRFileChangesAPI(
              owner,
              repo,
              item.number
            );

            if (fileChanges) {
              fileChanges.files = applyPartialContentFilter(
                fileChanges.files,
                params
              ) as DiffEntry[];

              result.file_changes = fileChanges;
            }
          }
        }
      }
    } catch (error: unknown) {
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (files): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  if (params.withComments) {
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
      result.comments = await fetchPRComments(
        octokit,
        owner,
        repo,
        item.number
      );
    }
  }

  if (params.withCommits) {
    try {
      const { owner, repo } = normalizeOwnerRepo(params);
      if (owner && repo) {
        const commits = await fetchPRCommitsWithFiles(
          owner,
          repo,
          item.number,
          params
        );
        if (commits) {
          result.commits = commits;
        }
      }
    } catch (error: unknown) {
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return result;
}

async function fetchPRFileChangesAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<{ total_count: number; files: DiffEntry[] } | null> {
  const octokit = await getOctokit(authInfo);
  const allFiles: DiffEntry[] = [];
  let page = 1;
  let keepFetching = true;

  do {
    const result = await octokit.rest.pulls.listFiles({
      owner,
      repo,
      pull_number: prNumber,
      per_page: 100,
      page: page,
    });

    allFiles.push(...result.data);
    keepFetching = result.data.length === 100;
    page++;
  } while (keepFetching);

  return {
    total_count: allFiles.length,
    files: allFiles,
  };
}

interface CommitListItem {
  sha: string;
  commit: {
    message: string;
    author: {
      name: string;
      date: string;
    } | null;
  };
}

async function fetchPRCommitsAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<CommitListItem[] | null> {
  const octokit = await getOctokit(authInfo);
  const result = await octokit.rest.pulls.listCommits({
    owner,
    repo,
    pull_number: prNumber,
  });

  return result.data as CommitListItem[];
}

async function fetchCommitFilesAPI(
  owner: string,
  repo: string,
  sha: string,
  authInfo?: AuthInfo
): Promise<CommitFileInfo[] | null> {
  try {
    const octokit = await getOctokit(authInfo);
    const result = await octokit.rest.repos.getCommit({
      owner,
      repo,
      ref: sha,
    });

    return (result.data.files || []) as CommitFileInfo[];
  } catch {
    return null;
  }
}

async function fetchPRCommitsWithFiles(
  owner: string,
  repo: string,
  prNumber: number,
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo
): Promise<CommitInfo[] | null> {
  const commits = await fetchPRCommitsAPI(owner, repo, prNumber, authInfo);
  if (!commits) return null;

  const sortedCommits = [...commits].sort((a, b) => {
    const dateA = a.commit.author?.date
      ? new Date(a.commit.author.date).getTime()
      : 0;
    const dateB = b.commit.author?.date
      ? new Date(b.commit.author.date).getTime()
      : 0;
    return dateB - dateA;
  });

  const commitInfos: CommitInfo[] = await Promise.all(
    sortedCommits.map(async commit => {
      const files = await fetchCommitFilesAPI(
        owner,
        repo,
        commit.sha,
        authInfo
      );

      let processedFiles: CommitInfo['files'] = [];

      if (files) {
        processedFiles = applyPartialContentFilter(
          files,
          params
        ) as CommitFileInfo[];
      }

      return {
        sha: commit.sha,
        message: commit.commit.message,
        author: commit.commit.author?.name || 'unknown',
        date: commit.commit.author?.date || '',
        files: processedFiles,
      };
    })
  );

  return commitInfos;
}

export async function transformPullRequestItemFromREST(
  item: PullRequestSimple | PullRequestItem,
  params: GitHubPullRequestsSearchParams,
  octokit: InstanceType<typeof OctokitWithThrottling>,
  authInfo?: AuthInfo
): Promise<GitHubPullRequestItem> {
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(item);

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  const type = params.type || 'metadata';
  const shouldFetchContent =
    type === 'fullContent' || type === 'partialContent' || type === 'metadata';

  // Owner and repo are guaranteed to be strings for REST API calls
  const owner = params.owner as string;
  const repo = params.repo as string;

  if (shouldFetchContent) {
    try {
      const fileChanges = await fetchPRFileChangesAPI(
        owner,
        repo,
        item.number,
        authInfo
      );
      if (fileChanges) {
        fileChanges.files = applyPartialContentFilter(
          fileChanges.files,
          params
        ) as DiffEntry[];
        result.file_changes = fileChanges;
      }
    } catch (error: unknown) {
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (files): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  if (params.withComments) {
    result.comments = await fetchPRComments(octokit, owner, repo, item.number);
  }

  if (params.withCommits) {
    try {
      const commits = await fetchPRCommitsWithFiles(
        owner,
        repo,
        item.number,
        params,
        authInfo
      );
      if (commits) {
        result.commits = commits;
      }
    } catch (error: unknown) {
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return result;
}
