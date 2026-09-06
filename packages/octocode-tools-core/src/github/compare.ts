import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse, HistoryCommitFile } from './githubAPI.js';
import { shapeCommitDirFiles } from './history/commitFiles.js';
import { MAX_PAGE_NUMBER } from '../config.js';
import { generateCacheKey } from '../utils/http/cache/key.js';
import { withDataCache } from '../utils/http/cache/dataCache.js';
import { resolveMaterializationRef } from './directoryFetch/refResolution.js';

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
  pagination?: {
    currentPage: number;
    perPage: number;
    hasMore: boolean;
    nextPage?: number;
  };
  isPartial?: boolean;
  terminalLimit?: boolean;
  partialReasons?: string[];
  providerLimit?: { reason: string; maxFiles?: number; maxPage?: number };
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

async function comparisonIdentity(
  data: {
    permalink_url?: string;
    base_commit?: { sha?: string };
    commits?: Array<{ sha?: string }>;
  },
  params: { owner: string; repo: string; base: string; head: string },
  authInfo?: AuthInfo
) {
  let references: RegExpMatchArray | null = null;
  if (data.permalink_url) {
    try {
      const pair = decodeURIComponent(
        new URL(data.permalink_url).pathname.match(
          /\/compare\/([^/]+)$/
        )?.[1] ?? ''
      );
      references = pair.match(
        /^((?:[\w.-]+:)?[a-f\d]{7,40})\.\.\.((?:[\w.-]+:)?[a-f\d]{7,40})$/i
      );
    } catch {
      /* A provider without a usable permalink retains the requested identity. */
    }
  }
  if (!references) return { base: params.base, head: params.head };
  const known = [
    ...new Set(
      [
        data.base_commit?.sha,
        ...(data.commits ?? []).map(commit => commit.sha),
        params.base.split(':').at(-1),
        params.head.split(':').at(-1),
      ]
        .filter(
          (sha): sha is string =>
            typeof sha === 'string' && /^[a-f\d]{40}$/i.test(sha)
        )
        .map(sha => sha.toLowerCase())
    ),
  ];
  const resolve = async (reference: string) => {
    const abbreviation = reference.split(':').at(-1)!.toLowerCase();
    const prefix = reference.slice(0, -abbreviation.length);
    if (abbreviation.length === 40) return prefix + abbreviation;
    const matches = known.filter(sha => sha.startsWith(abbreviation));
    const sha =
      matches.length === 1
        ? matches[0]!
        : (
            await resolveMaterializationRef(
              params.owner,
              params.repo,
              abbreviation,
              authInfo
            )
          ).commitSha;
    return prefix + sha;
  };
  const [base, head] = await Promise.all([
    resolve(references[1]!),
    resolve(references[2]!),
  ]);
  return { base, head };
}

/**
 * Exact history comparison: diff two refs (base...head) via GitHub's
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
    page?: number;
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
    const page = params.page ?? 1;
    const perPage = Math.min(100, Math.max(1, params.itemsPerPage ?? 30));
    const request = {
      owner: params.owner,
      repo: params.repo,
      basehead: `${params.base}...${params.head}`,
      per_page: perPage,
      page,
    };
    const auth = await resolveCacheAuthFingerprint(authInfo);
    const key = generateCacheKey('gh-compare-page', { ...request, auth });
    // Cache provider pages, independently of local file and patch windows.
    const resp = await withDataCache(key, () =>
      octokit.rest.repos.compareCommitsWithBasehead(request)
    );
    const d = resp.data;
    const identity = await comparisonIdentity(d, params, authInfo);
    const commits = (d.commits ?? []).map(c => ({
      sha: c.sha,
      messageHeadline: (c.commit.message ?? '').split('\n')[0] ?? '',
      author: c.commit.author?.name ?? c.author?.login ?? 'unknown',
      date: c.commit.author?.date ?? '',
    }));
    const allFiles = d.files ?? [];
    const hasMore =
      resp.headers?.link?.includes('rel="next"') === true ||
      page * perPage < d.total_commits;
    const fileLimit = allFiles.length >= 300;
    const pageLimit = hasMore && page >= MAX_PAGE_NUMBER;
    // When a path is given, scope the diff to that file so a large commit is
    // searchable by target instead of dumping every patch.
    const scopedFiles = params.path
      ? allFiles.filter(f => f.filename === params.path)
      : allFiles;
    let diffPayload:
      | Pick<CompareResult, 'files' | 'filesPagination'>
      | {
          changedFiles?: number;
        };
    if (params.includeDiff && page === 1) {
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
      diffPayload = page === 1 ? { changedFiles: scopedFiles.length } : {};
    }
    return {
      data: {
        type: 'compare',
        owner: params.owner,
        repo: params.repo,
        ...identity,
        status: d.status,
        aheadBy: d.ahead_by,
        behindBy: d.behind_by,
        totalCommits: d.total_commits,
        commits,
        pagination: {
          currentPage: page,
          perPage,
          hasMore,
          ...(hasMore && !pageLimit ? { nextPage: page + 1 } : {}),
        },
        ...(hasMore || fileLimit ? { isPartial: true } : {}),
        ...(fileLimit || pageLimit
          ? {
              terminalLimit: true,
              partialReasons: [
                ...(fileLimit ? ['providerFileLimit'] : []),
                ...(pageLimit ? ['schemaPageLimit'] : []),
              ],
              providerLimit: {
                reason: fileLimit ? 'providerFileLimit' : 'schemaPageLimit',
                ...(fileLimit ? { maxFiles: 300 } : {}),
                ...(pageLimit ? { maxPage: MAX_PAGE_NUMBER } : {}),
              },
            }
          : {}),
        ...diffPayload,
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
