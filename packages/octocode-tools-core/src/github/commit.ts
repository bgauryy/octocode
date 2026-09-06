import type { AuthInfo } from '@modelcontextprotocol/server';

import { COMMIT_FILE_LIMIT, fetchCommitDetail } from './commitDetail.js';
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
  isPartial?: boolean;
  terminalLimit?: boolean;
  partialReasons?: string[];
  providerLimit?: { reason: string; maxFiles: number };
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
    fileBatch?: number;
    itemsPerPage?: number;
    charOffset?: number;
    charLength?: number;
  },
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<ExactCommitResult>> {
  try {
    const response = await fetchCommitDetail(
      {
        owner: params.owner,
        repo: params.repo,
        ref: params.ref,
        fileBatch: params.fileBatch,
      },
      authInfo
    );
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
      changedFilesCountScope: 'providerBatch' as const,
      ...(response.collectionState.hasMore
        ? { isPartial: true, partialReasons: ['providerBatch'] }
        : {}),
      ...(response.terminalLimit
        ? {
            isPartial: true,
            terminalLimit: true,
            partialReasons: ['providerFileLimit'],
            providerLimit: {
              reason: 'providerFileLimit',
              maxFiles: COMMIT_FILE_LIMIT,
            },
          }
        : {}),
    };

    if (!params.includeDiff) {
      return {
        data: {
          ...base,
          ...(response.collectionState.hasMore
            ? {
                filesPagination: {
                  currentPage: 1,
                  totalPages: 1,
                  itemsPerPage: 100,
                  totalFiles: scopedFiles.length,
                  hasMore: true,
                  nextFilePage: 1,
                },
              }
            : {}),
        },
        status: 200,
      };
    }

    const shaped = shapeCommitDirFiles(scopedFiles, {
      filePage: params.filePage,
      itemsPerPage: params.itemsPerPage,
      charOffset: params.charOffset,
      charLength: params.charLength ?? 12_000,
    });
    const filesPagination = {
      ...shaped.filesPagination,
      countScope: 'providerBatch' as const,
      fileBatch: response.collectionState.page,
      ...(!shaped.filesPagination.hasMore && response.collectionState.hasMore
        ? {
            hasMore: true,
            nextFilePage: 1,
            nextFileBatch: response.collectionState.page + 1,
          }
        : {}),
    };
    return { data: { ...base, ...shaped, filesPagination }, status: 200 };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
