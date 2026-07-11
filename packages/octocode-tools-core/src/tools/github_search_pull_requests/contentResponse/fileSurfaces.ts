import { PR_CONTENT_DEFAULT_ITEMS_PER_PAGE } from '../../../config.js';
import type { NormalizedPrContentRequest } from '../contentRequest.js';
import { buildDiffPreview } from '../../../utils/parsers/diff.js';
import {
  containsNeedle,
  matchStringNeedle,
  paginateItems,
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
    ...(includePatch && typeof change.patch === 'string'
      ? { patch: change.patch }
      : {}),
  };
}

export function stripPatchComments(patch: string): string {
  return patch
    .split('\n')
    .filter(line => {
      if (!line.startsWith('+')) return true;
      const code = line.slice(1).trim();
      return (
        code !== '' &&
        !code.startsWith('//') &&
        !code.startsWith('/*') &&
        !code.startsWith('*')
      );
    })
    .map(line => {
      if (!line.startsWith('+')) return line;
      const code = line.slice(1);
      const stripped = code.replace(/\s*\/\/.*$/, '');
      return '+' + stripped.trimEnd();
    })
    .join('\n');
}

export function shapeFileSurfaces(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  shouldMinify?: boolean
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
  const { items, pagination } = paginateItems(
    matched,
    query.filePage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
  );

  const includePatch = request.patches.mode !== 'none';
  const shaped = items.map(change => {
    const base = shapeFileChange(change, false);
    if (!includePatch || typeof change.patch !== 'string') return base;
    const rawPatch =
      shouldMinify && !needle ? stripPatchComments(change.patch) : change.patch;
    const patch = paginateText(
      rawPatch,
      query.charOffset ?? 0,
      query.charLength ?? 12_000
    );
    return {
      ...base,
      patch: patch?.content ?? '',
      diff: buildDiffPreview(patch?.content),
      ...(patch ? { patchPagination: patch.pagination } : {}),
    };
  });

  if (request.changedFiles || request.patches.mode !== 'none') {
    return {
      changedFiles: shaped,
      filePagination: pagination,
    };
  }

  if (allChanges.length === 0) return {};

  return {
    filePathsPreview: allChanges.slice(0, 20).map(filePathOf).filter(Boolean),
    filePathsPagination: {
      totalFiles: allChanges.length,
      filesPerPage: 20,
      hasMore: allChanges.length > 20,
      ...(allChanges.length > 20 ? { nextFilePage: 2 } : {}),
    },
  };
}
