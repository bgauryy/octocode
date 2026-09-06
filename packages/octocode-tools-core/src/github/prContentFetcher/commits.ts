import {
  GitHubPullRequestsSearchParams,
  CommitInfo,
  DiffEntry,
  CommitFileInfo,
  PRProviderLimit,
} from '../githubAPI.js';
import { getOctokit } from '../client.js';
import { AuthInfo } from '@modelcontextprotocol/server';
import {
  fetchCollectionPage,
  type CollectionState,
} from './collectionPaging.js';
import { PR_CONTENT_DEFAULT_ITEMS_PER_PAGE } from '../../config.js';
import { COMMIT_FILE_LIMIT, fetchCommitDetail } from '../commitDetail.js';
import {
  attachRawResponseChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

type WithProviderLimits<T> = T & {
  providerLimits?: PRProviderLimit[];
  collectionState?: CollectionState;
};
const PR_COMMIT_LIMIT = 250;
const PR_FILE_LIMIT = 3000;

export async function fetchPRFileChangesAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo,
  collectionPage = 1
): Promise<WithProviderLimits<{
  total_count: number;
  files: DiffEntry[];
}> | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars, collectionState } =
    await fetchCollectionPage<DiffEntry>(
      { owner, repo, prNumber, surface: 'changedFiles' },
      collectionPage,
      page =>
        octokit.rest.pulls.listFiles({
          owner,
          repo,
          pull_number: prNumber,
          per_page: 100,
          page,
        }),
      authInfo
    );
  const atLimit =
    collectionPage * 100 >= PR_FILE_LIMIT &&
    (items.length >= 100 || collectionState.hasMore);
  const boundedState = {
    ...collectionState,
    hasMore: collectionState.hasMore && !atLimit,
  };

  return attachRawResponseChars(
    {
      collectionState: boundedState,
      total_count: items.length,
      files: [...items],
      ...(atLimit
        ? {
            providerLimits: [
              {
                reason: 'providerResultCap' as const,
                surface: 'changedFiles' as const,
                maxResults: PR_FILE_LIMIT,
              },
            ],
          }
        : {}),
    },
    rawResponseChars
  );
}

interface CommitListItem {
  sha: string;
  commit: {
    message: string;
    author: {
      name?: string;
      date?: string;
    } | null;
  };
}

export async function fetchPRCommitsAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo,
  collectionPage = 1
): Promise<WithProviderLimits<CommitListItem[]> | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars, collectionState } =
    await fetchCollectionPage<CommitListItem>(
      { owner, repo, prNumber, surface: 'commits' },
      collectionPage,
      page =>
        octokit.rest.pulls.listCommits({
          owner,
          repo,
          pull_number: prNumber,
          per_page: 50,
          page,
        }),
      authInfo
    );
  const atLimit =
    collectionPage * 50 >= PR_COMMIT_LIMIT &&
    (items.length >= 50 || collectionState.hasMore);
  const boundedState = {
    ...collectionState,
    hasMore: collectionState.hasMore && !atLimit,
  };

  // GitHub may stop at the cap without a next link. Treat equality
  // conservatively: the endpoint alone cannot establish completeness.
  const result: WithProviderLimits<CommitListItem[]> = Object.assign(
    [...items],
    { collectionState: boundedState }
  );
  if (atLimit)
    result.providerLimits = [
      {
        reason: 'providerResultCap',
        surface: 'commits',
        maxResults: PR_COMMIT_LIMIT,
      },
    ];
  return attachRawResponseChars(result, rawResponseChars);
}

export async function fetchCommitFilesAPI(
  owner: string,
  repo: string,
  sha: string,
  authInfo?: AuthInfo
): Promise<WithProviderLimits<CommitFileInfo[]> | null> {
  const result = await fetchCommitDetail(
    {
      owner,
      repo,
      ref: sha,
    },
    authInfo
  );

  const files: WithProviderLimits<CommitFileInfo[]> = [
    ...(result.data.files || []),
  ];
  if (result.terminalLimit)
    files.providerLimits = [
      {
        reason: 'providerResultCap',
        surface: 'commitFiles',
        maxResults: COMMIT_FILE_LIMIT,
        sha,
      },
    ];

  return attachRawResponseChars(
    Object.assign(files, { collectionState: result.collectionState }),
    result.rawResponseChars
  );
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
): Promise<WithProviderLimits<CommitInfo[]> | null> {
  const commits = await fetchPRCommitsAPI(
    owner,
    repo,
    prNumber,
    authInfo,
    params.collectionPages?.commits ?? 1
  );
  if (!commits) return null;

  let rawResponseChars = getRawResponseChars(commits) ?? 0;
  // Preserve provider order across batches; sorting one batch by author dates
  // would imply a chronology that does not hold across the full PR.
  const includeFiles =
    (params.content as { commits?: { includeFiles?: boolean } } | undefined)
      ?.commits?.includeFiles === true;
  const pageSize = Math.min(
    Math.max(1, params.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE),
    100
  );
  const totalPages = Math.max(1, Math.ceil(commits.length / pageSize));
  const page = Math.min(
    Math.max(1, params.commitPage ?? params.page ?? 1),
    totalPages
  );
  const pageStart = (page - 1) * pageSize;

  const providerLimits = [...(commits.providerLimits ?? [])];
  const commitInfos: WithProviderLimits<CommitInfo[]> = await mapPool(
    commits,
    COMMIT_FILES_CONCURRENCY,
    async (commit, index) => {
      // Retain every summary so the response shaper owns pagination and
      // total counts. Only commits visible on this page need enrichment.
      const files =
        includeFiles && index >= pageStart && index < pageStart + pageSize
          ? await fetchCommitFilesAPI(owner, repo, commit.sha, authInfo)
          : null;

      let processedFiles: CommitInfo['files'] = [];

      if (files) {
        providerLimits.push(...(files.providerLimits ?? []));
        rawResponseChars += getRawResponseChars(files) ?? 0;
        processedFiles = files;
      }

      return {
        sha: commit.sha,
        message: commit.commit.message,
        author: commit.commit.author?.name || 'unknown',
        date: commit.commit.author?.date || '',
        files: processedFiles,
        filesCollectionState: files?.collectionState,
      };
    }
  );

  commitInfos.collectionState = commits.collectionState;
  if (providerLimits.length) commitInfos.providerLimits = providerLimits;
  return attachRawResponseChars(commitInfos, rawResponseChars);
}
