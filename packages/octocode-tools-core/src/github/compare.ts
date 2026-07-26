import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse } from './githubAPI.js';

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
  /** Per-file diffs when includeDiff is on. */
  files?: Array<{
    filename: string;
    status: string;
    additions: number;
    deletions: number;
    patch?: string;
  }>;
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
    const files = d.files ?? [];
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
        ...(params.includeDiff
          ? {
              files: files.map(f => ({
                filename: f.filename,
                status: f.status,
                additions: f.additions,
                deletions: f.deletions,
                ...(f.patch ? { patch: f.patch } : {}),
              })),
            }
          : { changedFiles: files.length }),
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
