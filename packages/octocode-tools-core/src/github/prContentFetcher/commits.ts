import {
  GitHubPullRequestsSearchParams,
  CommitInfo,
  DiffEntry,
  CommitFileInfo,
} from '../githubAPI.js';
import { getOctokit } from '../client.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { applyPartialContentFilter } from '../prTransformation.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

export async function fetchAllPaginated<T>(
  fetchPage: (page: number) => Promise<{ data: T[] }>
): Promise<{ items: T[]; rawResponseChars: number }> {
  const items: T[] = [];
  let rawResponseChars = 0;
  let page = 1;
  let keepFetching = true;

  do {
    const result = await fetchPage(page);
    rawResponseChars += countSerializedChars(result.data);
    items.push(...result.data);
    keepFetching = result.data.length === 100;
    page++;
  } while (keepFetching);

  return { items, rawResponseChars };
}

export async function fetchPRFileChangesAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<{ total_count: number; files: DiffEntry[] } | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars } = await fetchAllPaginated<DiffEntry>(
    page =>
      octokit.rest.pulls.listFiles({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      }) as Promise<{ data: DiffEntry[] }>
  );

  return attachRawResponseChars(
    {
      total_count: items.length,
      files: items,
    },
    rawResponseChars
  );
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

export async function fetchPRCommitsAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<CommitListItem[] | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars } = await fetchAllPaginated<CommitListItem>(
    page =>
      octokit.rest.pulls.listCommits({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      }) as Promise<{ data: CommitListItem[] }>
  );

  return attachRawResponseChars(items, rawResponseChars);
}

export async function fetchCommitFilesAPI(
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

    return attachRawResponseChars(
      (result.data.files || []) as CommitFileInfo[],
      result.data
    );
  } catch {
    return null;
  }
}

const COMMIT_FILES_CONCURRENCY = 5;

export async function mapPool<T, R>(
  items: T[],
  concurrency: number,
  mapper: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  if (items.length === 0) return [];
  const results: R[] = new Array(items.length);
  let nextIndex = 0;
  const workers = Array.from(
    { length: Math.min(concurrency, items.length) },
    async () => {
      while (true) {
        const i = nextIndex++;
        if (i >= items.length) break;
        results[i] = await mapper(items[i]!, i);
      }
    }
  );
  await Promise.all(workers);
  return results;
}

export async function fetchPRCommitsWithFiles(
  owner: string,
  repo: string,
  prNumber: number,
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo
): Promise<CommitInfo[] | null> {
  const commits = await fetchPRCommitsAPI(owner, repo, prNumber, authInfo);
  if (!commits) return null;

  let rawResponseChars = getRawResponseChars(commits) ?? 0;
  const sortedCommits = [...commits].sort((a, b) => {
    const dateA = a.commit.author?.date
      ? new Date(a.commit.author.date).getTime()
      : 0;
    const dateB = b.commit.author?.date
      ? new Date(b.commit.author.date).getTime()
      : 0;
    return dateB - dateA;
  });

  const commitInfos: CommitInfo[] = await mapPool(
    sortedCommits,
    COMMIT_FILES_CONCURRENCY,
    async commit => {
      const files = await fetchCommitFilesAPI(
        owner,
        repo,
        commit.sha,
        authInfo
      );

      let processedFiles: CommitInfo['files'] = [];

      if (files) {
        rawResponseChars += getRawResponseChars(files) ?? 0;
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
    }
  );

  return attachRawResponseChars(commitInfos, rawResponseChars);
}
