import type { AuthInfo } from '@modelcontextprotocol/server';

import { getOctokit } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse, HistoryCommitFile } from './githubAPI.js';
import { shapeCommitDirFiles } from './history/commitFiles.js';

export interface ExactCommitResult {
  type: 'commit';
  owner: string;
  repo: string;
  ref: string;
  sha: string;
  message: string;
  messageHeadline: string;
  author: { name: string; email: string; login?: string; date?: string };
  committer?: { name: string; email: string; login?: string; date?: string };
  parents: string[];
  additions?: number;
  deletions?: number;
  changedFiles: number;
  files?: HistoryCommitFile[];
  filesPagination?: {
    currentPage: number;
    totalPages: number;
    itemsPerPage: number;
    totalFiles: number;
    hasMore: boolean;
    nextFilePage?: number;
  };
}

/** Retrieve one commit exactly by SHA/ref, optionally with paged/windowed diffs. */
export async function fetchCommit(
  params: {
    owner: string;
    repo: string;
    ref: string;
    includeDiff?: boolean;
    path?: string;
    filePage?: number;
    itemsPerPage?: number;
    charOffset?: number;
    charLength?: number;
  },
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<ExactCommitResult>> {
  try {
    const octokit = await getOctokit(authInfo);
    const response = await octokit.rest.repos.getCommit({
      owner: params.owner,
      repo: params.repo,
      ref: params.ref,
    });
    const commit = response.data;
    const allFiles = commit.files ?? [];
    const path = params.path;
    const scopedFiles = path
      ? allFiles.filter(
          file =>
            file.filename === path ||
            file.previous_filename === path ||
            file.filename.startsWith(path.endsWith('/') ? path : `${path}/`)
        )
      : allFiles;
    const author = commit.commit.author;
    const committer = commit.commit.committer;
    const authorLogin = commit.author?.login;
    const committerLogin = commit.committer?.login;
    const message = commit.commit.message;
    const base = {
      type: 'commit' as const,
      owner: params.owner,
      repo: params.repo,
      ref: params.ref,
      sha: commit.sha,
      message,
      messageHeadline: message.split('\n')[0] ?? message,
      author: {
        name: author?.name ?? 'unknown',
        email: author?.email ?? '',
        ...(authorLogin ? { login: authorLogin } : {}),
        ...(author?.date ? { date: author.date } : {}),
      },
      ...(committer
        ? {
            committer: {
              name: committer.name ?? 'unknown',
              email: committer.email ?? '',
              ...(committerLogin ? { login: committerLogin } : {}),
              ...(committer.date ? { date: committer.date } : {}),
            },
          }
        : {}),
      parents: commit.parents.map(parent => parent.sha),
      ...(commit.stats?.additions === undefined
        ? {}
        : { additions: commit.stats.additions }),
      ...(commit.stats?.deletions === undefined
        ? {}
        : { deletions: commit.stats.deletions }),
      changedFiles: scopedFiles.length,
    };

    if (!params.includeDiff) {
      return { data: base, status: 200 };
    }

    return {
      data: {
        ...base,
        ...shapeCommitDirFiles(scopedFiles, {
          filePage: params.filePage,
          itemsPerPage: params.itemsPerPage,
          charOffset: params.charOffset,
          charLength: params.charLength,
        }),
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
