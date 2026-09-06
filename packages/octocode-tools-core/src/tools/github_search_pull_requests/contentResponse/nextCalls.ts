import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { baseQuery, type QueryLike } from './pagination.js';

function withTargetContent(
  target: Record<string, unknown>,
  content: Record<string, unknown>
): Record<string, unknown> {
  return {
    tool: 'ghGetHistoryItem',
    query: { ...target, content },
    confidence: 'exact',
  };
}

export function nextCalls(
  query: QueryLike,
  prNumber: number,
  request: NormalizedPrContentRequest,
  firstChangedFilePath?: string
) {
  const target = baseQuery(query, prNumber);
  return {
    ...(request.body
      ? {}
      : { getBody: withTargetContent(target, { body: true }) }),
    ...(request.changedFiles
      ? {}
      : {
          getChangedFiles: withTargetContent(target, { changedFiles: true }),
        }),
    ...(request.patches.mode !== 'none'
      ? {}
      : {
          ...(firstChangedFilePath
            ? {
                getSelectedPatches: withTargetContent(target, {
                  patches: { mode: 'selected', files: [firstChangedFilePath] },
                }),
              }
            : {}),
          getAllPatches: withTargetContent(target, {
            patches: { mode: 'all' },
          }),
        }),
    ...(request.comments
      ? {}
      : {
          getComments: withTargetContent(target, {
            comments: { discussion: true, reviewInline: true },
          }),
        }),
    ...(request.reviews
      ? {}
      : { getReviews: withTargetContent(target, { reviews: true }) }),
    ...(request.commits
      ? {}
      : {
          getCommits: withTargetContent(target, { commits: {} }),
        }),
    fullReview: withTargetContent(target, {
      body: true,
      changedFiles: true,
      patches: { mode: 'all' },
      comments: { discussion: true, reviewInline: true },
      reviews: true,
      commits: {},
    }),
  };
}
