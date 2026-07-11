import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { buildDiffPreview } from '../utils/parsers/diff.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import type {
  GitHubAPIResponse,
  HistoryCommit,
  HistoryCommitFile,
  HistoryResult,
} from './githubAPI.js';

/** Cap parallel repos.getCommit calls when includeDiff is true. */
const COMMIT_DIFF_CONCURRENCY = 5;

/** GitHub REST pagination: a `rel="next"` Link header means more pages exist. */
export function parseHasMore(linkHeader: string | undefined): boolean {
  if (!linkHeader) return false;
  return linkHeader.includes('rel="next"');
}

async function mapPool<T, R>(
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
        const index = nextIndex++;
        if (index >= items.length) return;
        results[index] = await mapper(items[index] as T, index);
      }
    }
  );
  await Promise.all(workers);
  return results;
}

function windowPatch(
  patch: string | undefined,
  charOffset: number | undefined,
  charLength: number | undefined
):
  | {
      patch: string;
      patchPagination?: {
        charOffset: number;
        charLength: number;
        totalChars: number;
        hasMore: boolean;
        nextCharOffset?: number;
      };
    }
  | undefined {
  if (!patch) return undefined;
  if (!charLength && !charOffset) return { patch };

  const totalChars = patch.length;
  const start = Math.min(Math.max(0, charOffset ?? 0), totalChars);
  const length = Math.max(1, charLength ?? totalChars);
  const end = Math.min(start + length, totalChars);
  const hasMore = end < totalChars;
  return {
    patch: patch.slice(start, end),
    patchPagination: {
      charOffset: start,
      charLength: end - start,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: end } : {}),
    },
  };
}

type FetchHistoryParams = {
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
  filePage?: number;
  itemsPerPage?: number;
  includeDiff: boolean;
  charOffset?: number;
  charLength?: number;
};

export async function fetchHistory(
  params: FetchHistoryParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<HistoryResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-history',
    {
      type: params.type,
      owner: params.owner,
      repo: params.repo,
      path: params.path,
      branch: params.branch,
      since: params.since,
      until: params.until,
      author: params.author,
      page: params.page,
      perPage: params.perPage,
      filePage: params.filePage,
      itemsPerPage: params.itemsPerPage,
      includeDiff: params.includeDiff,
      charOffset: params.charOffset,
      charLength: params.charLength,
      auth,
    },
    sessionId
  );

  return withDataCache<GitHubAPIResponse<HistoryResult>>(
    cacheKey,
    () => fetchHistoryInternal(params, authInfo),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchHistoryInternal(
  params: FetchHistoryParams,
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
      const fullMessage = item.commit.message;
      const messageHeadline = fullMessage.split('\n')[0] ?? fullMessage;
      // Token trim: headline is the default payload; the body is included only
      // when it adds information, capped at 500 chars (benchmark measured
      // multi-KB commit trailers dominating one-shot history answers).
      const body = fullMessage.slice(messageHeadline.length).trim();
      const bodyTruncated = body.length > 500;
      const message =
        body.length === 0
          ? messageHeadline
          : `${messageHeadline}\n${bodyTruncated ? `${body.slice(0, 500)}…` : body}`;
      // web-flow is GitHub's merge-UI bot — a constant boilerplate committer
      // on every merged commit; it carries no research signal.
      const committerSameAsAuthor =
        (committerObj?.name === authorObj?.name &&
          committerObj?.email === authorObj?.email) ||
        item.committer?.login === 'web-flow';

      return {
        sha: item.sha,
        date,
        ...(message === messageHeadline ? {} : { message }),
        ...(bodyTruncated ? { messageTruncated: true as const } : {}),
        messageHeadline,
        url: item.html_url,
        author: {
          name: authorObj?.name ?? 'unknown',
          email: authorObj?.email ?? '',
          ...(item.author?.login ? { login: item.author.login } : {}),
        },
        ...(committerObj && !committerSameAsAuthor
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
      currentPage: params.page,
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

    // Phase 2: fetch per-commit diffs with bounded concurrency — non-fatal
    const commitsWithDiff = await mapPool(
      baseCommits,
      COMMIT_DIFF_CONCURRENCY,
      async (commit, idx) => {
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
              const patchWindow =
                fileData.patch !== undefined
                  ? windowPatch(
                      fileData.patch,
                      params.charOffset,
                      params.charLength
                    )
                  : undefined;
              return {
                ...commit,
                additions: fileData.additions,
                deletions: fileData.deletions,
                status: fileData.status,
                ...(patchWindow !== undefined
                  ? {
                      patch: patchWindow.patch,
                      ...(patchWindow.patchPagination
                        ? { patchPagination: patchWindow.patchPagination }
                        : {}),
                      diff: buildDiffPreview(patchWindow.patch),
                    }
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
              .map(f => {
                const patchWindow =
                  f.patch !== undefined
                    ? windowPatch(f.patch, params.charOffset, params.charLength)
                    : undefined;
                return {
                  filename: f.filename,
                  status: f.status,
                  additions: f.additions,
                  deletions: f.deletions,
                  ...(patchWindow !== undefined
                    ? {
                        patch: patchWindow.patch,
                        ...(patchWindow.patchPagination
                          ? { patchPagination: patchWindow.patchPagination }
                          : {}),
                        diff: buildDiffPreview(patchWindow.patch),
                      }
                    : {}),
                  ...(f.previous_filename
                    ? { previousFilename: f.previous_filename }
                    : {}),
                };
              });
            const filePage = Math.max(1, params.filePage ?? 1);
            const itemsPerPage = Math.max(1, params.itemsPerPage ?? 20);
            const totalFiles = allFiles.length;
            const totalPages = Math.max(
              1,
              Math.ceil(totalFiles / itemsPerPage)
            );
            const currentPage = Math.min(filePage, totalPages);
            const start = (currentPage - 1) * itemsPerPage;
            const files = allFiles.slice(start, start + itemsPerPage);
            return {
              ...commit,
              files,
              filesPagination: {
                currentPage,
                totalPages,
                itemsPerPage,
                totalFiles,
                hasMore: currentPage < totalPages,
                ...(currentPage < totalPages
                  ? { nextFilePage: currentPage + 1 }
                  : {}),
              },
            };
          }
        } catch {
          // diff fetch is non-fatal — return base commit without diff
        }
        return commit;
      }
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
