import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type {
  GitHubAPIResponse,
  HistoryCommit,
  HistoryCommitFile,
  HistoryResult,
} from './githubAPI.js';

function parseHasMore(linkHeader: string | undefined): boolean {
  if (!linkHeader) return false;
  return linkHeader.includes('rel="next"');
}

function truncatePatch(
  patch: string | undefined,
  charLength: number | undefined
): string | undefined {
  if (!patch) return undefined;
  if (!charLength || patch.length <= charLength) return patch;
  return patch.slice(0, charLength);
}

export async function fetchHistory(
  params: {
    type: 'file' | 'repo';
    owner: string;
    repo: string;
    path?: string;
    branch?: string;
    since?: string;
    until?: string;
    author?: string;
    page: number;
    perPage: number;
    includeDiff: boolean;
    charLength?: number;
  },
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<HistoryResult>> {
  try {
    const octokit = await getOctokit(authInfo);

    const listParams = {
      owner: params.owner,
      repo: params.repo,
      per_page: params.perPage,
      page: params.page,
      ...(params.path ? { path: params.path } : {}),
      ...(params.branch ? { sha: params.branch } : {}),
      ...(params.since ? { since: params.since } : {}),
      ...(params.until ? { until: params.until } : {}),
      ...(params.author ? { author: params.author } : {}),
    };

    const response = await octokit.rest.repos.listCommits(listParams);

    const linkHeader = response.headers.link as string | undefined;
    const hasMore = parseHasMore(linkHeader);

    const baseCommits: HistoryCommit[] = response.data.map(item => {
      const authorObj = item.commit.author;
      const committerObj = item.commit.committer;
      const date = committerObj?.date ?? authorObj?.date ?? '';
      const message = item.commit.message;
      const messageHeadline = message.split('\n')[0] ?? message;

      return {
        sha: item.sha,
        date,
        message,
        messageHeadline,
        url: item.html_url,
        author: {
          name: authorObj?.name ?? 'unknown',
          email: authorObj?.email ?? '',
          ...(item.author?.login ? { login: item.author.login } : {}),
        },
        ...(committerObj
          ? {
              committer: {
                name: committerObj.name ?? 'unknown',
                email: committerObj.email ?? '',
                ...(item.committer?.login
                  ? { login: item.committer.login }
                  : {}),
              },
            }
          : {}),
      };
    });

    const pagination = {
      page: params.page,
      perPage: params.perPage,
      hasMore,
      ...(hasMore ? { nextPage: params.page + 1 } : {}),
    };

    if (!params.includeDiff) {
      return {
        data: {
          type: params.type,
          owner: params.owner,
          repo: params.repo,
          ...(params.path ? { path: params.path } : {}),
          commits: baseCommits,
          pagination,
        },
        status: 200,
      };
    }

    // Phase 2: fetch per-commit diffs in parallel — non-fatal on failure
    const commitsWithDiff = await Promise.all(
      baseCommits.map(async (commit, idx) => {
        try {
          const sha = response.data[idx]?.sha ?? commit.sha;
          const detail = await octokit.rest.repos.getCommit({
            owner: params.owner,
            repo: params.repo,
            ref: sha,
          });

          if (params.type === 'file' && params.path) {
            const filePath = params.path;
            const fileData = detail.data.files?.find(
              f => f.filename === filePath || f.previous_filename === filePath
            );
            if (fileData) {
              return {
                ...commit,
                additions: fileData.additions,
                deletions: fileData.deletions,
                status: fileData.status,
                ...(fileData.patch !== undefined
                  ? { patch: truncatePatch(fileData.patch, params.charLength) }
                  : {}),
                ...(fileData.previous_filename
                  ? { previousFilename: fileData.previous_filename }
                  : {}),
              };
            }
          } else {
            // type: "repo" — return all changed files (filtered to dir prefix if set)
            const dirPath = params.path;
            const allFiles: HistoryCommitFile[] = (detail.data.files ?? [])
              .filter(f => !dirPath || f.filename.startsWith(dirPath))
              .map(f => ({
                filename: f.filename,
                status: f.status,
                additions: f.additions,
                deletions: f.deletions,
                ...(f.patch !== undefined
                  ? { patch: truncatePatch(f.patch, params.charLength) }
                  : {}),
                ...(f.previous_filename
                  ? { previousFilename: f.previous_filename }
                  : {}),
              }));
            return { ...commit, files: allFiles };
          }
        } catch {
          // diff fetch is non-fatal — return base commit without diff
        }
        return commit;
      })
    );

    return {
      data: {
        type: params.type,
        owner: params.owner,
        repo: params.repo,
        ...(params.path ? { path: params.path } : {}),
        commits: commitsWithDiff,
        pagination,
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
