import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { parseHasMore } from './history.js';
import type { GitHubAPIResponse } from './githubAPI.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';

type ReleaseAsset = {
  name: string;
  size: number;
  downloadCount: number;
  url: string;
};

type ReleaseRow = {
  tagName: string;
  name?: string;
  publishedAt?: string;
  prerelease?: true;
  draft?: true;
  /** Marks the release GitHub reports as "latest" (stable, non-prerelease). */
  latest?: true;
  /** Downloadable assets (opt-in via includeAssets). */
  assets?: ReleaseAsset[];
};

export type ReleasesResult = {
  type: 'releases';
  owner: string;
  repo: string;
  releases: ReleaseRow[];
  /** The repo's latest stable release, surfaced even when it is not on this page. */
  latest?: { tagName: string; publishedAt?: string };
  pagination: {
    currentPage: number;
    perPage: number;
    hasMore: boolean;
    nextPage?: number;
  };
};

type FetchReleasesParams = {
  owner: string;
  repo: string;
  page: number;
  perPage: number;
  includeAssets?: boolean;
};

export async function fetchReleases(
  params: FetchReleasesParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<ReleasesResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-releases',
    {
      owner: params.owner,
      repo: params.repo,
      page: params.page,
      perPage: params.perPage,
      includeAssets: params.includeAssets === true,
      auth,
    },
    sessionId
  );

  return withDataCache<GitHubAPIResponse<ReleasesResult>>(
    cacheKey,
    () => fetchReleasesInternal(params, authInfo),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchReleasesInternal(
  params: FetchReleasesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<ReleasesResult>> {
  try {
    const octokit = await getOctokit(authInfo);

    const [listResponse, latestResponse] = await Promise.all([
      octokit.rest.repos.listReleases({
        owner: params.owner,
        repo: params.repo,
        per_page: params.perPage,
        page: params.page,
      }),
      // 404 here just means the repo has no stable release — non-fatal.
      octokit.rest.repos
        .getLatestRelease({ owner: params.owner, repo: params.repo })
        .catch(() => undefined),
    ]);

    const latestData = latestResponse?.data;
    const releases: ReleaseRow[] = listResponse.data.map(r => ({
      tagName: r.tag_name,
      ...(r.name && r.name !== r.tag_name ? { name: r.name } : {}),
      ...(r.published_at ? { publishedAt: r.published_at } : {}),
      ...(r.prerelease ? { prerelease: true as const } : {}),
      ...(r.draft ? { draft: true as const } : {}),
      ...(latestData !== undefined && r.id === latestData.id
        ? { latest: true as const }
        : {}),
      ...(params.includeAssets && r.assets && r.assets.length > 0
        ? {
            assets: r.assets.map(a => ({
              name: a.name,
              size: a.size,
              downloadCount: a.download_count,
              url: a.browser_download_url,
            })),
          }
        : {}),
    }));

    const hasMore = parseHasMore(
      listResponse.headers.link as string | undefined
    );

    return {
      data: {
        type: 'releases',
        owner: params.owner,
        repo: params.repo,
        releases,
        ...(latestData
          ? {
              latest: {
                tagName: latestData.tag_name,
                ...(latestData.published_at
                  ? { publishedAt: latestData.published_at }
                  : {}),
              },
            }
          : {}),
        pagination: {
          currentPage: params.page,
          perPage: params.perPage,
          hasMore,
          ...(hasMore ? { nextPage: params.page + 1 } : {}),
        },
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
