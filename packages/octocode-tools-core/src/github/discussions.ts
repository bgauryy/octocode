import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import type { GitHubAPIResponse } from './githubAPI.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';

type DiscussionRow = {
  number: number;
  title: string;
  url: string;
  author?: string;
  category?: string;
  createdAt?: string;
  updatedAt?: string;
  /** True when a comment has been accepted as the answer (Q&A categories). */
  answered?: true;
  upvotes?: number;
  comments?: number;
};

export type DiscussionsResult = {
  type: 'discussions';
  owner: string;
  repo: string;
  totalCount: number;
  discussions: DiscussionRow[];
  pagination: {
    perPage: number;
    hasMore: boolean;
    /** Opaque cursor to pass back as `after` for the next page. */
    nextCursor?: string;
    continuationUnavailable?: {
      reason: 'missingProviderCursor';
    };
  };
  /**
   * Whether the repository has Discussions enabled at all. Omitted (not
   * `false`) when the repo-level probe itself failed/was inconclusive — a
   * `totalCount:0` response is otherwise ambiguous between "disabled" and
   * "enabled, nothing posted yet", which this field disambiguates without a
   * second call.
   */
  hasDiscussionsEnabled?: boolean;
};

type FetchDiscussionsParams = {
  owner: string;
  repo: string;
  keywords?: string[];
  perPage: number;
  after?: string;
};

// GitHub Discussions have no REST list endpoint — GraphQL search is the only
// way to page + text-search them in one call. `repo:` scopes to the repository;
// extra terms match title/body.
const DISCUSSIONS_QUERY = `
query($q: String!, $first: Int!, $after: String, $owner: String!, $repo: String!) {
  search(query: $q, type: DISCUSSION, first: $first, after: $after) {
    discussionCount
    pageInfo { hasNextPage endCursor }
    nodes {
      ... on Discussion {
        number
        title
        url
        createdAt
        updatedAt
        answerChosenAt
        upvoteCount
        author { login }
        category { name }
        comments { totalCount }
      }
    }
  }
  repository(owner: $owner, name: $repo) {
    hasDiscussionsEnabled
  }
}`;

type GraphQLDiscussionNode = {
  number: number;
  title: string;
  url: string;
  createdAt?: string;
  updatedAt?: string;
  answerChosenAt?: string | null;
  upvoteCount?: number;
  author?: { login?: string } | null;
  category?: { name?: string } | null;
  comments?: { totalCount?: number };
};

type GraphQLDiscussionsResponse = {
  search: {
    discussionCount: number;
    pageInfo: { hasNextPage: boolean; endCursor?: string | null };
    nodes: Array<GraphQLDiscussionNode | Record<string, never>>;
  };
  repository?: { hasDiscussionsEnabled?: boolean } | null;
};

export async function fetchDiscussions(
  params: FetchDiscussionsParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<DiscussionsResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey(
    'gh-api-discussions',
    {
      owner: params.owner,
      repo: params.repo,
      keywords: (params.keywords ?? []).join(' '),
      perPage: params.perPage,
      after: params.after ?? '',
      auth,
    },
    sessionId
  );

  return withDataCache<GitHubAPIResponse<DiscussionsResult>>(
    cacheKey,
    () => fetchDiscussionsInternal(params, authInfo),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchDiscussionsInternal(
  params: FetchDiscussionsParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<DiscussionsResult>> {
  try {
    const octokit = await getOctokit(authInfo);

    const terms = (params.keywords ?? [])
      .map(k => k.trim())
      .filter(Boolean)
      .join(' ');
    const q = `repo:${params.owner}/${params.repo}${terms ? ` ${terms}` : ''}`;

    const result = await octokit.graphql<GraphQLDiscussionsResponse>(
      DISCUSSIONS_QUERY,
      {
        q,
        first: params.perPage,
        owner: params.owner,
        repo: params.repo,
        ...(params.after ? { after: params.after } : {}),
      }
    );

    const nodes = (result.search.nodes ?? []).filter(
      (n): n is GraphQLDiscussionNode =>
        typeof (n as GraphQLDiscussionNode).number === 'number'
    );

    const discussions: DiscussionRow[] = nodes.map(n => ({
      number: n.number,
      title: n.title,
      url: n.url,
      ...(n.author?.login ? { author: n.author.login } : {}),
      ...(n.category?.name ? { category: n.category.name } : {}),
      ...(n.createdAt ? { createdAt: n.createdAt } : {}),
      ...(n.updatedAt ? { updatedAt: n.updatedAt } : {}),
      ...(n.answerChosenAt ? { answered: true as const } : {}),
      ...(typeof n.upvoteCount === 'number' ? { upvotes: n.upvoteCount } : {}),
      ...(typeof n.comments?.totalCount === 'number'
        ? { comments: n.comments.totalCount }
        : {}),
    }));

    const hasMore = result.search.pageInfo.hasNextPage === true;
    const endCursor = result.search.pageInfo.endCursor ?? undefined;

    return {
      data: {
        type: 'discussions',
        owner: params.owner,
        repo: params.repo,
        totalCount: result.search.discussionCount,
        discussions,
        pagination: {
          perPage: params.perPage,
          hasMore,
          ...(hasMore && endCursor ? { nextCursor: endCursor } : {}),
          ...(hasMore && !endCursor
            ? {
                continuationUnavailable: {
                  reason: 'missingProviderCursor' as const,
                },
              }
            : {}),
        },
        ...(typeof result.repository?.hasDiscussionsEnabled === 'boolean'
          ? { hasDiscussionsEnabled: result.repository.hasDiscussionsEnabled }
          : {}),
      },
      status: 200,
    };
  } catch (error) {
    return handleGitHubAPIError(error);
  }
}
