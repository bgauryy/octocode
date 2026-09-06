import { patchAvailability } from '../../../github/history/commitFiles.js';
import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { historyPatchView } from './contentView.js';
import {
  containsNeedle,
  matchStringNeedle,
  paginateCollection,
  paginateText,
  type QueryLike,
} from './pagination.js';

export function filePathOf(change: Record<string, unknown>): string {
  return String(change.path ?? change.filename ?? '');
}

export function shapeFileChange(
  change: Record<string, unknown>,
  includePatch: boolean
) {
  return {
    path: filePathOf(change),
    status: String(change.status ?? ''),
    additions: Number(change.additions ?? 0),
    deletions: Number(change.deletions ?? 0),
    ...(includePatch ? patchAvailability(change.patch) : {}),
    ...(includePatch && typeof change.patch === 'string'
      ? { patch: change.patch }
      : {}),
  };
}

export function shapeFileSurfaces(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  const allChanges = Array.isArray(pr.fileChanges)
    ? (pr.fileChanges as Array<Record<string, unknown>>)
    : [];
  const files = request.patches.files;
  const selected =
    files && files.length > 0
      ? allChanges.filter(change => files.includes(filePathOf(change)))
      : allChanges;
  const needle = matchStringNeedle(query);
  const matched = needle
    ? selected.filter(
        change =>
          containsNeedle(filePathOf(change), needle) ||
          containsNeedle(change.patch, needle)
      )
    : selected;
  const { items, pagination } = paginateCollection(
    matched,
    query,
    pr,
    'changedFiles',
    query.filePage ?? query.page ?? 1
  );

  const includePatch = request.patches.mode !== 'none';
  const shaped = items.map(change => {
    const base = shapeFileChange(change, includePatch);
    if (!includePatch || typeof change.patch !== 'string') return base;
    const patch = paginateText(
      historyPatchView(change.patch, query),
      query.charOffset ?? 0,
      query.charLength ?? 12_000
    );
    return {
      ...base,
      patch: patch?.content ?? '',
      ...(patch ? { patchPagination: patch.pagination } : {}),
    };
  });

  if (request.changedFiles || request.patches.mode !== 'none') {
    return {
      changedFiles: shaped,
      contentPagination: { changedFiles: pagination },
    };
  }

  if (allChanges.length === 0) return {};

  return {
    filePathsPreview: allChanges.slice(0, 20).map(filePathOf).filter(Boolean),
    contentPagination: {
      filePaths: {
        totalFiles: allChanges.length,
        filesPerPage: 20,
        hasMore: allChanges.length > 20,
        ...(allChanges.length > 20 ? { nextFilePage: 2 } : {}),
      },
    },
  };
}
