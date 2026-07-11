import { PR_CONTENT_DEFAULT_ITEMS_PER_PAGE } from '../../../config.js';
import type { NormalizedPrContentRequest } from '../contentRequest.js';
import {
  compactBody,
  containsNeedle,
  matchStringNeedle,
  paginateItems,
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
  const { items, pagination } = paginateItems(
    matched,
    query.commentPage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
  );
  return {
    comments: items.map(comment => {
      const body = paginateText(
        typeof comment.body === 'string' ? comment.body : '',
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
    commentPagination: pagination,
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
  return {
    reviews: reviews.map(review => {
      const rawBody = typeof review.body === 'string' ? review.body : '';
      const paginated = paginateText(
        rawBody || undefined,
        0,
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
  const { items, pagination } = paginateItems(
    allCommits,
    query.commitPage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
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
        ? { files: commit.files }
        : {}),
    })),
    commitPagination: pagination,
  };
}
