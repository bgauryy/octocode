import type {
  ProviderResponse,
  PullRequestItem,
  PullRequestQuery,
  PullRequestSearchResult,
} from '../types.js';
import {
  fetchBitbucketPRSupplementalData,
  searchBitbucketPRsAPI,
} from '../../bitbucket/pullRequestSearch.js';
import { handleBitbucketAPIError } from '../../bitbucket/errors.js';
import type {
  BitbucketDiffstatEntry,
  BitbucketPRComment,
  BitbucketPullRequest,
} from '../../bitbucket/types.js';
import {
  handleBitbucketAPIResponse,
  parseBitbucketProjectId,
} from './utils.js';
import {
  parseUnifiedDiffByFile,
  shapePullRequestFileChanges,
} from '../pullRequestFileChanges.js';

type BitbucketPRState =
  | 'OPEN'
  | 'MERGED'
  | 'DECLINED'
  | 'SUPERSEDED'
  | undefined;

interface BitbucketPullRequestWithDetails extends BitbucketPullRequest {
  _comments?: BitbucketPRComment[];
  _diffstat?: BitbucketDiffstatEntry[];
  _diff?: string;
}

const BITBUCKET_PR_STATE_MAP: Record<
  string,
  Exclude<BitbucketPRState, undefined>
> = {
  open: 'OPEN',
  opened: 'OPEN',
  closed: 'DECLINED',
  declined: 'DECLINED',
  merged: 'MERGED',
  superseded: 'SUPERSEDED',
};

const VALID_BITBUCKET_PR_STATES = [
  'open',
  'closed',
  'merged',
  'superseded',
  'all',
] as const;

function createInvalidPRStateError(state: string): Error {
  return new Error(
    `Invalid Bitbucket PR state '${state}'. Valid values are: ${VALID_BITBUCKET_PR_STATES.join(', ')}.`
  );
}

export function mapPRState(state?: string): BitbucketPRState {
  if (!state) {
    return undefined;
  }

  const normalizedState = state.trim().toLowerCase();

  if (!normalizedState) {
    throw createInvalidPRStateError(state);
  }

  if (normalizedState === 'all') {
    return undefined;
  }

  const mapped = BITBUCKET_PR_STATE_MAP[normalizedState];
  if (!mapped) {
    throw createInvalidPRStateError(state);
  }
  return mapped;
}

function mapUnifiedState(bbState: string): 'open' | 'closed' | 'merged' {
  if (bbState === 'MERGED') return 'merged';
  if (bbState === 'DECLINED' || bbState === 'SUPERSEDED') return 'closed';
  return 'open';
}

export function transformPullRequestResult(
  pullRequests: BitbucketPullRequest[],
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    totalMatches?: number;
  },
  comments?: BitbucketPRComment[],
  diffstat?: BitbucketDiffstatEntry[],
  diff?: string,
  query: PullRequestQuery = {}
): PullRequestSearchResult {
  const pullRequestsWithDetails = pullRequests.map((pullRequest, index) => {
    if (index === 0 && (comments || diffstat || diff)) {
      return {
        ...pullRequest,
        _comments: comments,
        _diffstat: diffstat,
        _diff: diff,
      } satisfies BitbucketPullRequestWithDetails;
    }

    return pullRequest as BitbucketPullRequestWithDetails;
  });

  const items: PullRequestItem[] = pullRequestsWithDetails.map(pr => {
    const patchesByPath = parseUnifiedDiffByFile(pr._diff);
    const rawFileChanges =
      pr._diffstat?.map(fileChange => {
        const path = fileChange.new?.path || fileChange.old?.path || '';
        return {
          path,
          status: fileChange.status,
          additions: fileChange.lines_added,
          deletions: fileChange.lines_removed,
          patch: patchesByPath.get(path),
        };
      }) || [];

    const fileChangeSummary = shapePullRequestFileChanges(
      rawFileChanges,
      query
    );

    return {
      number: pr.id,
      title: pr.title,
      body: pr.description ?? null,
      url: pr.links?.html?.href || '',
      state: mapUnifiedState(pr.state),
      draft: false,
      author: pr.author?.username || pr.author?.display_name || '',
      assignees: [],
      labels: [],
      sourceBranch: pr.source?.branch?.name || '',
      targetBranch: pr.destination?.branch?.name || '',
      sourceSha: pr.source?.commit?.hash,
      targetSha: pr.destination?.commit?.hash,
      createdAt: pr.created_on || '',
      updatedAt: pr.updated_on || '',
      closedAt: undefined,
      mergedAt: pr.state === 'MERGED' ? pr.updated_on : undefined,
      commentsCount: pr.comment_count,
      comments: pr._comments?.map(comment => ({
        id: String(comment.id),
        author: comment.user?.username || comment.user?.display_name || '',
        body: comment.content?.raw || '',
        createdAt: comment.created_on || '',
        updatedAt: comment.updated_on || '',
      })),
      ...fileChangeSummary,
    };
  });

  return {
    items,
    totalCount: pagination.totalMatches || items.length,
    pagination,
  };
}

export async function searchPullRequests(
  query: PullRequestQuery
): Promise<ProviderResponse<PullRequestSearchResult>> {
  if (!query.projectId) {
    return {
      error:
        'Project ID (workspace/repo_slug) is required for Bitbucket PR search.',
      status: 400,
      provider: 'bitbucket',
    };
  }

  const needsFileChanges = query.type !== undefined;
  let state: BitbucketPRState;
  let workspace = '';
  let repoSlug = '';

  try {
    state = mapPRState(query.state);
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? error.message
          : 'Invalid Bitbucket PR state provided.',
      status: 400,
      provider: 'bitbucket',
      hints: [
        'Valid states are: open, closed, merged, superseded, all.',
        'Use "all" to return all states.',
      ],
    };
  }

  try {
    ({ workspace, repoSlug } = parseBitbucketProjectId(query.projectId));
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? error.message
          : 'Invalid Bitbucket project ID provided.',
      status: 400,
      provider: 'bitbucket',
      hints: ['Provide projectId as "workspace/repo_slug".'],
    };
  }

  const result = await searchBitbucketPRsAPI({
    workspace,
    repoSlug,
    prNumber: query.number,
    state,
    author: query.author,
    baseBranch: query.baseBranch,
    headBranch: query.headBranch,
    sort: query.sort,
    page: query.page,
    limit: query.limit,
    withComments: query.withComments,
    withDiff: query.type === 'fullContent',
    withDiffstat: needsFileChanges,
  });

  const providerResult = handleBitbucketAPIResponse(result, data => data);

  if (!providerResult.data) {
    return {
      error: providerResult.error || 'No data returned from Bitbucket API',
      status: providerResult.status,
      provider: 'bitbucket',
      hints: providerResult.hints,
      rateLimit: providerResult.rateLimit,
    };
  }

  let pullRequests = providerResult.data
    .pullRequests as BitbucketPullRequestWithDetails[];

  if (
    !query.number &&
    pullRequests.length > 0 &&
    (query.withComments || needsFileChanges)
  ) {
    pullRequests = await Promise.all(
      pullRequests.map(async pullRequest => {
        try {
          const supplementalData = await fetchBitbucketPRSupplementalData({
            workspace,
            repoSlug,
            prNumber: pullRequest.id,
            withComments: query.withComments,
            withDiff: query.type === 'fullContent',
            withDiffstat: needsFileChanges,
          });

          return {
            ...pullRequest,
            _comments: supplementalData.comments,
            _diffstat: supplementalData.diffstat,
            _diff: supplementalData.diff,
          };
        } catch (error) {
          handleBitbucketAPIResponse(
            handleBitbucketAPIError(error),
            data => data
          );
          return pullRequest;
        }
      })
    );
  }

  return {
    data: transformPullRequestResult(
      pullRequests,
      providerResult.data.pagination,
      providerResult.data.comments,
      providerResult.data.diffstat,
      providerResult.data.diff,
      query
    ),
    status: providerResult.status,
    provider: 'bitbucket',
    hints: providerResult.hints,
    rateLimit: providerResult.rateLimit,
  };
}
