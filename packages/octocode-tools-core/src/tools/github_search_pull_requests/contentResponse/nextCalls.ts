import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { baseQuery, type QueryLike } from './pagination.js';

// Every fragment merges `target` (owner/repo/prNumber) into its own query so
// each one is copy-paste ready on its own — the fragment used to be a
// content-only delta an agent had to manually merge with a separate `target`
// object before the call would actually run.
function withTargetContent(
  target: Record<string, unknown>,
  content: Record<string, unknown>
): Record<string, unknown> {
  return { ...target, content };
}

function withTargetFields(
  target: Record<string, unknown>,
  fields: Record<string, unknown>
): Record<string, unknown> {
  return { ...target, ...fields };
}

export function nextCalls(
  query: QueryLike,
  prNumber: number,
  request: NormalizedPrContentRequest,
  firstChangedFilePath?: string
) {
  const target = baseQuery(query, prNumber);
  return {
    target,
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
          // A real path is only known once changedFiles has actually been
          // fetched in this same response; otherwise fall back to a labeled
          // placeholder that still names what to substitute.
          getSelectedPatches: withTargetContent(target, {
            patches: {
              mode: 'selected',
              files: [firstChangedFilePath ?? 'path/from/changedFiles'],
            },
          }),
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
          getCommits: withTargetContent(target, { commits: { list: true } }),
        }),
    ...(request.reviewMode === 'full'
      ? {}
      : { fullReview: withTargetFields(target, { reviewMode: 'full' }) }),
  };
}
