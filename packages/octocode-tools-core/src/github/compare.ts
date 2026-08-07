import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse, HistoryCommitFile } from './githubAPI.js';
import { shapeCommitDirFiles } from './history/commitFiles.js';

export type CompareResult = {
  type: 'compare';
  owner: string;
  repo: string;
  base: string;
  head: string;
  /** ahead | behind | diverged | identical */
  status: string;
  aheadBy: number;
  behindBy: number;
  totalCommits: number;
  commits: Array<{
    sha: string;
    messageHeadline: string;
    author: string;
    date: string;
  }>;
  /** File count when includeDiff is off (lean default). */
  changedFiles?: number;
  /** Per-file diffs when includeDiff is on (windowed + paginated). */
  files?: HistoryCommitFile[];
  /** File-list pagination when includeDiff is on. */
  filesPagination?: {
    currentPage: number;
    totalPages: number;
    itemsPerPage: number;
    totalFiles: number;
    hasMore: boolean;
    nextFilePage?: number;
  };
};

/**
 * Compare mode of ghSearchCommits: diff two refs (base...head) via GitHub's
 * compare endpoint. Returns ahead/behind counts, the commits between them, and
 * a lean changed-file count (or full diffs when includeDiff is set).
 */
export async function compareRefs(
  params: {
    owner: string;
    repo: string;
    base: string;
    head: string;
    includeDiff?: boolean;
    /** Restrict the diff to a single file path (searchable scope). */
    path?: string;
    filePage?: number;
    itemsPerPage?: number;
    charOffset?: number;
    charLength?: number;
  },
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<CompareResult>> {
  try {
    const octokit = await getOctokit(authInfo);
    const resp = await octokit.rest.repos.compareCommitsWithBasehead({
      owner: params.owner,
      repo: params.repo,
      basehead: `${params.base}...${params.head}`,
    });
    const d = resp.data;
    const commits = (d.commits ?? []).map(c => ({
      sha: c.sha,
      messageHeadline: (c.commit.message ?? '').split('\n')[0] ?? '',
      author: c.commit.author?.name ?? c.author?.login ?? 'unknown',
      date: c.commit.author?.date ?? '',
    }));
    const allFiles = d.files ?? [];
    // When a path is given, scope the diff to that file so a large commit is
    // searchable by target instead of dumping every patch.
    const scopedFiles = params.path
      ? allFiles.filter(f => f.filename === params.path)
      : allFiles;
    let diffPayload:
      | Pick<CompareResult, 'files' | 'filesPagination'>
      | {
          changedFiles: number;
        };
    if (params.includeDiff) {
      const shaped = shapeCommitDirFiles(scopedFiles, {
        filePage: params.filePage,
        itemsPerPage: params.itemsPerPage,
        charOffset: params.charOffset,
        charLength: params.charLength,
      });
      diffPayload = {
        files: shaped.files,
        filesPagination: shaped.filesPagination,
      };
    } else {
      diffPayload = { changedFiles: scopedFiles.length };
    }
    return {
      data: {
        type: 'compare',
        owner: params.owner,
        repo: params.repo,
        base: params.base,
        head: params.head,
        status: d.status,
        aheadBy: d.ahead_by,
        behindBy: d.behind_by,
        totalCommits: d.total_commits,
        commits,
        ...diffPayload,
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
