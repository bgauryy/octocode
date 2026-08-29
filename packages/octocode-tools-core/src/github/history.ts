import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { resolveDateWindow } from './dateWindow.js';
import { handleGitHubAPIError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import type {
  GitHubAPIResponse,
  HistoryCommit,
  HistoryResult,
} from './githubAPI.js';
import { shapeCommitDirFiles, windowPatch } from './history/commitFiles.js';

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

type FetchHistoryParams = {
  type: 'file' | 'repo';
  owner: string;
  repo: string;
  path?: string;
  branch?: string;
  since?: string;
  until?: string;
  author?: string;
  committer?: string;
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
      committer: params.committer,
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

    const dateWarnings: string[] = [];
    const sinceResolved = params.since
      ? resolveDateWindow(params.since)
      : undefined;
    const untilResolved = params.until
      ? resolveDateWindow(params.until)
      : undefined;
    if (sinceResolved?.warning)
      dateWarnings.push(`since ${sinceResolved.warning}`);
    if (untilResolved?.warning)
      dateWarnings.push(`until ${untilResolved.warning}`);

    const listParams = {
      owner: params.owner,
      repo: params.repo,
      per_page: params.perPage,
      page: params.page,
      ...(params.path ? { path: params.path } : {}),
      ...(params.branch ? { sha: params.branch } : {}),
      ...(sinceResolved?.value ? { since: sinceResolved.value } : {}),
      ...(untilResolved?.value ? { until: untilResolved.value } : {}),
      ...(params.author ? { author: params.author } : {}),
      ...(params.committer ? { committer: params.committer } : {}),
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

    // Empty walk under a date window reads as a false absence: GitHub's
    // since/until match the COMMITTER date (a commit authored inside the
    // window but merged/rebased later is excluded), and the path walk does
    // not follow renames. Say so instead of returning a bare empty.
    if (
      baseCommits.length === 0 &&
      (sinceResolved?.value !== undefined || untilResolved?.value !== undefined)
    ) {
      dateWarnings.push(
        'no commits matched the since/until window — GitHub filters by committer date (not author date; rebases and squash-merges reset it), and a path-scoped walk does not follow renames. Widen or drop since/until and inspect commit dates directly.'
      );
    }

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
          ...(dateWarnings.length ? { warnings: dateWarnings } : {}),
        },
        status: 200,
      };
    }

    // Phase 2: fetch per-commit diffs with bounded concurrency — non-fatal
    let dirFallbackUsed = false;
    let fileDiffMissing = false;
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
                    }
                  : {}),
                ...(fileData.previous_filename
                  ? { previousFilename: fileData.previous_filename }
                  : {}),
              };
            }
            // No file with that exact name — the caller's "file" path is
            // usually a DIRECTORY written without a trailing slash (the mode
            // classifier can't tell locally). Fall back to a dir-prefix
            // filter instead of silently returning the commit without a
            // diff; if nothing matches the prefix either, flag that too.
            const prefix = filePath.endsWith('/') ? filePath : `${filePath}/`;
            const dirFiles = (detail.data.files ?? []).filter(f =>
              f.filename.startsWith(prefix)
            );
            if (dirFiles.length > 0) {
              dirFallbackUsed = true;
              return {
                ...commit,
                ...shapeCommitDirFiles(dirFiles, params),
              };
            }
            fileDiffMissing = true;
            return commit;
          } else {
            // type: "repo" — return all changed files (filtered to dir prefix if set)
            const dirPath = params.path;
            const matching = (detail.data.files ?? []).filter(
              f => !dirPath || f.filename.startsWith(dirPath)
            );
            return {
              ...commit,
              ...shapeCommitDirFiles(matching, params),
            };
          }
        } catch {
          // diff fetch is non-fatal — return base commit without diff
        }
        return commit;
      }
    );

    const diffWarnings: string[] = [];
    if (dirFallbackUsed) {
      diffWarnings.push(
        `path '${params.path}' matched no single file in these commits but matches files under it — treated as a directory filter and returned per-commit changed files (append '/' to select directory mode explicitly).`
      );
    }
    if (fileDiffMissing) {
      diffWarnings.push(
        `includeDiff: some commits contain no file matching '${params.path}' (rename or shallow diff?) — those commits are listed without a diff.`
      );
    }
    const allWarnings = [...dateWarnings, ...diffWarnings];

    return {
      data: {
        type: params.type,
        owner: params.owner,
        repo: params.repo,
        ...(params.path ? { path: params.path } : {}),
        commits: commitsWithDiff,
        pagination,
        ...(allWarnings.length ? { warnings: allWarnings } : {}),
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
