import { GITHUB_GET_HISTORY_ITEM_TOOL_NAME } from '../toolNames.js';
import { publicCommitContinuationQuery } from './historyContinuations.js';

/** Advance file and patch axes independently so finishing one file never skips another. */
export function withDiffContinuations(
  data: Record<string, unknown>,
  query: Record<string, unknown>
): Record<string, unknown> {
  const filesPagination = data.filesPagination as
    { nextFilePage?: number; nextFileBatch?: number } | undefined;
  const pagination = data.pagination as { nextPage?: number } | undefined;
  const files = Array.isArray(data.files) ? data.files : [];
  const firstPatch = files.find(
    file => typeof file?.patchPagination?.nextCharOffset === 'number'
  );
  const next: Record<string, unknown> = {};
  const identity =
    query.operation === 'commit'
      ? publicCommitContinuationQuery(
          query,
          typeof data.sha === 'string' ? { ref: data.sha } : {}
        )
      : query.operation === 'compare' &&
          typeof data.base === 'string' &&
          typeof data.head === 'string'
        ? { ...query, base: data.base, head: data.head }
        : query;
  const continuation = (patch: Record<string, unknown>, why: string) => ({
    tool: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
    query: Object.fromEntries(
      Object.entries({ ...identity, ...patch }).filter(
        ([, value]) => value !== undefined
      )
    ),
    why,
    confidence: 'exact',
  });
  if (typeof pagination?.nextPage === 'number') {
    next.nextPage = continuation(
      { page: pagination.nextPage },
      'Continue the comparison commit list. Changed files are returned on page 1.'
    );
  }
  if (typeof filesPagination?.nextFilePage === 'number') {
    next.nextFilePage = continuation(
      {
        filePage: filesPagination.nextFilePage,
        ...(filesPagination.nextFileBatch
          ? { fileBatch: filesPagination.nextFileBatch }
          : {}),
        ...(query.operation === 'commit' ? { includeDiff: true } : {}),
        charOffset: undefined,
      },
      'Continue the changed-file list from the beginning of each new patch.'
    );
  }
  if (typeof firstPatch?.patchPagination?.nextCharOffset === 'number') {
    next.continuePatch = continuation(
      { charOffset: firstPatch.patchPagination.nextCharOffset },
      'Continue the current patch window.'
    );
  }
  return Object.keys(next).length > 0 ? { ...data, next } : data;
}
