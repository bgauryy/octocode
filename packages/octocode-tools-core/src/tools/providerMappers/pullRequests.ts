import type { PullRequestSearchResult as ProviderPullRequestSearchResult } from '../../providers/types.js';
import type { z } from 'zod';
import type { GitHubPullRequestSearchQueryLocalSchema } from '../github_search_pull_requests/scheme.js';
import type { WithOptionalMeta } from '../../types/execution.js';

import { GITHUB_SEARCH_DEFAULT_LIMIT } from '../../config.js';
import { quoteSearchKeyword } from '../../github/searchKeyword.js';
import { isBotAuthor } from '../../github/botFilter.js';
import { countMetadata, toProviderProjectId } from './shared.js';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;

type PRDefaultKeys = 'order' | 'limit' | 'page';
type PartialPRQuery = WithOptionalMeta<
  Omit<GitHubPullRequestSearchQuery, PRDefaultKeys> &
    Partial<Pick<GitHubPullRequestSearchQuery, PRDefaultKeys>>
>;

export function mapPullRequestToolQuery(query: PartialPRQuery) {
  const keywordParts = (query.keywordsToSearch ?? [])
    .filter(k => k.trim())
    .map(quoteSearchKeyword);
  const rawQuery = (query as { query?: string }).query?.trim() ?? '';
  const combinedQuery =
    [...keywordParts, ...(rawQuery ? [rawQuery] : [])].join(' ') || undefined;

  return {
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    query: combinedQuery,
    number: query.prNumber,
    state: query.state as 'open' | 'closed' | 'merged' | 'all' | undefined,
    author: query.author,
    assignee: query.assignee,
    commenter: query.commenter,
    involves: query.involves,
    mentions: query.mentions,
    reviewRequested: query['review-requested'],
    reviewedBy: query['reviewed-by'],
    labels: (() => {
      const labelValue = query.label;
      if (!labelValue) return undefined;
      return Array.isArray(labelValue) ? labelValue : [labelValue];
    })(),
    noLabel: query['no-label'],
    noMilestone: query['no-milestone'],
    noProject: query['no-project'],
    noAssignee: query['no-assignee'],
    baseBranch: query.base,
    headBranch: query.head,
    created: query.created,
    updated: query.updated,
    closed: query.closed,
    mergedAt: query['merged-at'],
    comments: query.comments,
    reactions: query.reactions,
    interactions: query.interactions,
    draft: query.draft,
    match: query.match,
    milestone: query.milestone,
    language: query.language,
    checks: query.checks,
    review: query.review,
    locked: query.locked,
    visibility: query.visibility,
    teamMentions: query['team-mentions'],
    project: query.project,
    archived: (query as Record<string, unknown>).archived as
      boolean | undefined,
    content: (query as { content?: unknown }).content,
    reviewMode: (query as { reviewMode?: 'summary' | 'full' }).reviewMode,
    filePage: (query as { filePage?: number }).filePage,
    commentPage: (query as { commentPage?: number }).commentPage,
    commitPage: (query as { commitPage?: number }).commitPage,
    itemsPerPage: (query as { itemsPerPage?: number }).itemsPerPage,
    sort: query.sort as
      | 'created'
      | 'updated'
      | 'best-match'
      | 'comments'
      | 'reactions'
      | undefined,
    order: query.order as 'asc' | 'desc' | undefined,
    limit: (query as { limit?: number }).limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
    page: query.page,
    charOffset: (query as { charOffset?: number }).charOffset,
    charLength: (query as { charLength?: number }).charLength,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

type ProviderPrComment = NonNullable<
  ProviderPullRequestSearchResult['items'][number]['comments']
>[number];
type ProviderPrReview = NonNullable<
  ProviderPullRequestSearchResult['items'][number]['reviews']
>[number];

// Review `state` (APPROVED/CHANGES_REQUESTED/COMMENTED/DISMISSED) is GitHub's
// own verdict — ground truth, unlike guessing intent from comment text. A
// single clean APPROVED review must never be mislabeled by a keyword that
// happens to appear in an unrelated bot comment (e.g. a size-check bot
// commenting "this PR **changes** 500 lines" was previously read as
// "changes-requested").
function detectReviewThemes(
  comments: readonly ProviderPrComment[],
  reviews: readonly ProviderPrReview[]
): string[] {
  const themes: string[] = [];

  if (reviews.some(review => review.state === 'APPROVED')) {
    themes.push('approval');
  }
  if (reviews.some(review => review.state === 'CHANGES_REQUESTED')) {
    themes.push('changes-requested');
  }

  // Comment-body heuristics are a weaker secondary signal — restricted to
  // human (non-bot) comments so automated tool output can't inject a theme.
  const humanBodies = comments
    .filter(comment => !isBotAuthor(comment.author ?? ''))
    .map(comment => comment.body.toLowerCase());

  if (
    themes.length === 0 &&
    humanBodies.some(body =>
      /\b(lgtm|looks good|approved|ship it)\b/.test(body)
    )
  ) {
    themes.push('approval');
  }
  if (
    !themes.includes('changes-requested') &&
    humanBodies.some(body =>
      /\b(change|fix|concern|blocker|blocking|request changes?)\b/.test(body)
    )
  ) {
    themes.push('changes-requested');
  }
  if (humanBodies.some(body => body.includes('?'))) {
    themes.push('question');
  }

  return themes.length > 0 ? themes : ['discussion'];
}

function buildReviewSummary(
  comments: readonly ProviderPrComment[] | undefined,
  reviews: readonly ProviderPrReview[] | undefined
):
  | {
      totalComments: number;
      inlineComments: number;
      discussionComments: number;
      commenters: string[];
      latestCommentAt?: string;
      themes: string[];
    }
  | undefined {
  if (!comments || comments.length === 0) return undefined;
  const commenters = Array.from(
    new Set(comments.map(comment => comment.author))
  );
  const latestCommentAt = comments
    .map(comment => comment.updatedAt || comment.createdAt)
    .filter(Boolean)
    .sort()
    .at(-1);
  const inlineComments = comments.filter(
    c =>
      (c as ProviderPrComment & { commentType?: string }).commentType ===
      'review_inline'
  ).length;
  return {
    totalComments: comments.length,
    inlineComments,
    discussionComments: comments.length - inlineComments,
    commenters: commenters.slice(0, 8),
    ...(latestCommentAt ? { latestCommentAt } : {}),
    themes: detectReviewThemes(comments, reviews ?? []),
  };
}

export function mapPullRequestProviderResultData(
  data: ProviderPullRequestSearchResult,
  options: { includeFileChanges?: boolean } = {}
) {
  const { includeFileChanges = true } = options;
  const pullRequests = data.items.map(pr => {
    const fileChanges = pr.fileChanges;
    const originalFileChangeCount = fileChanges?.length ?? 0;
    const comments = Array.isArray(pr.comments) ? pr.comments : undefined;
    const reviewSummary = buildReviewSummary(comments, pr.reviews);
    return {
      number: pr.number,
      title: pr.title,
      body: pr.body ?? undefined,
      ...(pr.bodyPagination && { bodyPagination: pr.bodyPagination }),
      url: pr.url,
      state: pr.state,
      draft: pr.draft,
      author: pr.author,
      assignees: pr.assignees,
      labels: pr.labels,
      sourceBranch: pr.sourceBranch,
      targetBranch: pr.targetBranch,
      sourceSha: pr.sourceSha,
      targetSha: pr.targetSha,
      createdAt: pr.createdAt,
      updatedAt: pr.updatedAt,
      closedAt: pr.closedAt,
      mergedAt: pr.mergedAt,
      commentsCount: pr.commentsCount,
      changedFilesCount: pr.changedFilesCount ?? originalFileChangeCount,
      additions: pr.additions,
      deletions: pr.deletions,
      ...(Array.isArray(pr.comments) &&
        pr.comments.length > 0 && {
          comments: pr.comments.map(comment => ({
            ...comment,
            ...(comment.bodyPagination && {
              bodyPagination: comment.bodyPagination,
            }),
          })),
        }),
      ...(pr.reviews && { reviews: pr.reviews }),
      ...(pr.commits && { commits: pr.commits }),
      ...(reviewSummary && { reviewSummary }),
      ...(fileChanges && includeFileChanges ? { fileChanges } : {}),
      ...(Array.isArray(pr.sanitizationWarnings) &&
      pr.sanitizationWarnings.length > 0
        ? { sanitizationWarnings: pr.sanitizationWarnings }
        : {}),
    };
  });

  const pagination = data.pagination
    ? {
        currentPage: data.pagination.currentPage,
        totalPages: data.pagination.totalPages,
        perPage: data.pagination.entriesPerPage || 10,
        ...(typeof data.pagination.totalMatches === 'number'
          ? { totalMatches: data.pagination.totalMatches }
          : {}),
        ...countMetadata(data.pagination),
        hasMore: data.pagination.hasMore,
        ...(data.pagination.hasMore
          ? { nextPage: data.pagination.currentPage + 1 }
          : {}),
      }
    : undefined;

  return {
    pullRequests,
    resultData: {
      pull_requests: pullRequests,
      ...(pagination
        ? { pagination }
        : { total_count: data.totalCount || pullRequests.length }),
    } as Record<string, unknown>,
    pagination,
  };
}
