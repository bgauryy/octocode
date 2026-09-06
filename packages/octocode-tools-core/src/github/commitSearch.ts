import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { withDataCache } from '../utils/http/cache/dataCache.js';
import { generateCacheKey } from '../utils/http/cache/key.js';
import { resolveDateWindow } from './dateWindow.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse } from './githubAPI.js';
import { countSerializedChars } from '../utils/response/charSavings.js';

interface CommitSearchParams {
  owner: string;
  repo: string;
  keywords: string[];
  author?: string;
  committer?: string;
  since?: string;
  until?: string;
  page: number;
  perPage: number;
}

export async function searchCommits(
  params: CommitSearchParams,
  authInfo?: AuthInfo
): Promise<
  GitHubAPIResponse<
    Record<string, unknown> & { commits: Array<Record<string, unknown>> }
  >
> {
  try {
    const { owner, repo, page, perPage } = params;
    if (!/^[\w.-]+$/.test(owner) || !/^[\w.-]+$/.test(repo)) {
      return {
        error: 'Commit search requires one repository owner and name.',
        type: 'http',
        status: 400,
      };
    }
    if ((page - 1) * perPage >= 1000) {
      return {
        error:
          'GitHub commit search exposes at most 1000 matches. Narrow keywords or date bounds before requesting this page.',
        type: 'http',
        status: 400,
      };
    }
    const since = params.since ? resolveDateWindow(params.since) : undefined;
    const until = params.until ? resolveDateWindow(params.until) : undefined;
    if (since?.warning || until?.warning) {
      return {
        error: `Invalid ${since?.warning ? 'since' : 'until'} date for commit search. Use an ISO date or relative window such as "30d".`,
        type: 'http',
        status: 400,
      };
    }
    const parts = [
      ...params.keywords.map(keyword => JSON.stringify(keyword)),
      `repo:${owner}/${repo}`,
    ];
    for (const field of ['author', 'committer'] as const) {
      const value = params[field];
      if (value)
        parts.push(
          `${field}${value.includes('@') ? '-email' : ''}:${JSON.stringify(value)}`
        );
    }
    if (since?.value || until?.value) {
      const range =
        since?.value && until?.value
          ? `${since.value}..${until.value}`
          : since?.value
            ? `>=${since.value}`
            : `<=${until!.value}`;
      parts.push(`committer-date:${range}`);
    }
    const octokit = await getOctokit(authInfo);
    const request = {
      q: parts.join(' '),
      page,
      per_page: perPage,
      sort: 'committer-date' as const,
      order: 'desc' as const,
    };
    const auth = await resolveCacheAuthFingerprint(authInfo);
    const response = await withDataCache(
      generateCacheKey('gh-commit-search', { ...request, auth }),
      () => octokit.rest.search.commits(request),
      { shouldCache: value => !value.data.incomplete_results }
    );
    const {
      items,
      total_count: totalCount,
      incomplete_results: incompleteResults,
    } = response.data;
    const commits = items.map(item => {
      const headline = item.commit.message.split('\n')[0] ?? '';
      const body = item.commit.message.slice(headline.length).trim();
      return {
        sha: item.sha,
        url: item.html_url,
        messageHeadline: headline,
        ...(body ? { message: `${headline}\n${body.slice(0, 500)}` } : {}),
        ...(body.length > 500 ? { messageTruncated: true } : {}),
        date: item.commit.committer?.date ?? item.commit.author?.date ?? '',
        author: {
          name: item.commit.author?.name ?? 'unknown',
          email: item.commit.author?.email ?? '',
          ...(item.author?.login ? { login: item.author.login } : {}),
        },
      };
    });
    const hasMore = page * perPage < Math.min(totalCount, 1000);
    return {
      status: 200,
      rawResponseChars: countSerializedChars(response.data),
      data: {
        type: 'commits',
        owner,
        repo,
        scope: 'defaultBranch',
        commits,
        totalCount,
        incompleteResults,
        pagination: {
          page,
          perPage,
          hasMore,
          ...(hasMore ? { nextPage: page + 1 } : {}),
          totalMatchesCapped: totalCount > 1000,
        },
      },
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
