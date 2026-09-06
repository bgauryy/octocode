import { shapeCommitDirFiles } from '../../../github/history/commitFiles.js';
import { withDiffContinuations } from '../historyDiffContinuations.js';
import type { CollectionState } from '../../../github/prContentFetcher/collectionPaging.js';
import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { historyBodyView } from './contentView.js';
import {
  compactBody,
  containsNeedle,
  matchStringNeedle,
  paginateCollection,
  paginateText,
  type QueryLike,
} from './pagination.js';

type CommentRequest = Exclude<NormalizedPrContentRequest['comments'], false>;

function filterComments(
  comments: Array<Record<string, unknown>>,
  request: CommentRequest
): Array<Record<string, unknown>> {
  return comments.filter(comment => {
    const type = comment.commentType;
    if (request.file && comment.path !== request.file) return false;
    if (type === 'review_inline') return request.reviewInline;
    return request.discussion;
  });
}

export function shapeComments(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.comments) return {};
  const allComments = Array.isArray(pr.comments)
    ? (pr.comments as Array<Record<string, unknown>>)
    : [];
  const filtered = filterComments(allComments, request.comments);
  const needle = matchStringNeedle(query);
  const matched = needle
    ? filtered.filter(comment => containsNeedle(comment.body, needle))
    : filtered;
  const { items, pagination } = paginateCollection(
    matched,
    query,
    pr,
    'comments',
    query.commentPage ?? query.page ?? 1
  );
  return {
    comments: items.map(comment => {
      const body = paginateText(
        historyBodyView(
          typeof comment.body === 'string' ? comment.body : '',
          query
        ),
        query.commentBodyOffset ?? 0,
        query.charLength ?? 12_000
      );
      return {
        id: comment.id,
        author: comment.author,
        commentType: comment.commentType ?? 'discussion',
        path: comment.path,
        line: comment.line,
        ...(comment.in_reply_to_id != null
          ? { in_reply_to_id: comment.in_reply_to_id }
          : {}),
        ...(body
          ? { body: body.content, bodyPagination: body.pagination }
          : {
              bodyPreview: compactBody(
                typeof comment.body === 'string' ? comment.body : ''
              ),
            }),
        createdAt: comment.createdAt,
        updatedAt: comment.updatedAt,
      };
    }),
    contentPagination: { comments: pagination },
  };
}

export function shapeReviews(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.reviews) return {};
  const allReviews = Array.isArray(pr.reviews)
    ? (pr.reviews as Array<Record<string, unknown>>)
    : [];
  const needle = matchStringNeedle(query);
  const reviews = needle
    ? allReviews.filter(review => containsNeedle(review.body, needle))
    : allReviews;
  const { items, pagination } = paginateCollection(
    reviews,
    query,
    pr,
    'reviews',
    query.reviewPage ?? 1
  );
  return {
    contentPagination: { reviews: pagination },
    reviews: items.map(review => {
      const rawBody = typeof review.body === 'string' ? review.body : '';
      const paginated = paginateText(
        rawBody ? historyBodyView(rawBody, query) : undefined,
        query.charOffset ?? 0,
        query.charLength ?? 12_000
      );
      return {
        id: review.id,
        user: review.user,
        state: review.state,
        ...(paginated
          ? { body: paginated.content, bodyPagination: paginated.pagination }
          : {}),
        submittedAt: review.submittedAt ?? review.submitted_at,
        commitId: review.commitId ?? review.commit_id,
      };
    }),
  };
}

export function shapeCommits(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.commits) return {};
  const allCommits = Array.isArray(pr.commits)
    ? (pr.commits as Array<Record<string, unknown>>)
    : [];
  const { items, pagination } = paginateCollection(
    allCommits,
    query,
    pr,
    'commits',
    query.commitPage ?? query.page ?? 1
  );
  return {
    commits: items.map(commit => ({
      sha: commit.sha,
      message: commit.message,
      author: commit.author,
      date: commit.date,
      ...(request.commits &&
      request.commits.includeFiles &&
      Array.isArray(commit.files)
        ? shapeNestedFiles(commit, query)
        : {}),
    })),
    contentPagination: { commits: pagination },
  };
}

function shapeNestedFiles(commit: Record<string, unknown>, query: QueryLike) {
  const shaped = shapeCommitDirFiles(
    commit.files as Parameters<typeof shapeCommitDirFiles>[0],
    {
      itemsPerPage: query.pageSize,
      charLength: query.charLength ?? 12_000,
    }
  );
  const state = commit.filesCollectionState as CollectionState | undefined;
  const filesPagination = {
    ...shaped.filesPagination,
    countScope: 'providerBatch',
    ...(!shaped.filesPagination.hasMore && state?.hasMore
      ? { hasMore: true, nextFilePage: 1, nextFileBatch: state.page + 1 }
      : {}),
  };
  return withDiffContinuations(
    { files: shaped.files, filesPagination },
    {
      operation: 'commit',
      owner: query.owner,
      repo: query.repo,
      ref: commit.sha,
      includeDiff: true,
      pageSize: query.pageSize,
      charLength: query.charLength,
    }
  );
}
