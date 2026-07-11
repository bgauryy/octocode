import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { baseQuery, type QueryLike } from './pagination.js';

export function nextCalls(
  query: QueryLike,
  prNumber: number,
  request: NormalizedPrContentRequest
) {
  return {
    target: baseQuery(query, prNumber),
    ...(request.body ? {} : { getBody: { content: { body: true } } }),
    ...(request.changedFiles
      ? {}
      : { getChangedFiles: { content: { changedFiles: true } } }),
    ...(request.patches.mode !== 'none'
      ? {}
      : {
          getSelectedPatches: {
            content: {
              patches: { mode: 'selected', files: ['path/from/changedFiles'] },
            },
          },
          getAllPatches: { content: { patches: { mode: 'all' } } },
        }),
    ...(request.comments
      ? {}
      : {
          getComments: {
            content: { comments: { discussion: true, reviewInline: true } },
          },
        }),
    ...(request.reviews ? {} : { getReviews: { content: { reviews: true } } }),
    ...(request.commits
      ? {}
      : { getCommits: { content: { commits: { list: true } } } }),
    ...(request.reviewMode === 'full'
      ? {}
      : { fullReview: { reviewMode: 'full' } }),
  };
}
