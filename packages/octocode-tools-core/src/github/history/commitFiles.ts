import type { HistoryCommitFile } from '../githubAPI.js';
import { MAX_PAGE_NUMBER } from '../../config.js';

/** A missing provider patch does not establish whether a file is binary or too large. */
export function patchAvailability(patch: unknown) {
  return typeof patch === 'string'
    ? {}
    : {
        isPartial: true as const,
        terminalLimit: true as const,
        patchUnavailable: { reason: 'providerOmittedPatch' as const },
      };
}

export function windowPatch(
  patch: string | undefined,
  charOffset: number | undefined,
  charLength: number | undefined
):
  | {
      patch: string;
      patchPagination?: {
        charOffset: number;
        charLength: number;
        totalChars: number;
        hasMore: boolean;
        nextCharOffset?: number;
      };
    }
  | undefined {
  if (patch === undefined) return undefined;
  if (!charLength && !charOffset) return { patch };

  const totalChars = patch.length;
  const start = Math.min(Math.max(0, charOffset ?? 0), totalChars);
  const length = Math.max(1, charLength ?? totalChars);
  const end = Math.min(start + length, totalChars);
  const hasMore = end < totalChars;
  return {
    patch: patch.slice(start, end),
    patchPagination: {
      charOffset: start,
      charLength: end - start,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: end } : {}),
    },
  };
}

/**
 * Shape a commit's raw changed-file rows into the windowed, paginated
 * `files` + `filesPagination` payload. Shared by repo/dir mode and the
 * file-mode directory fallback so both emit identical structure.
 */
export function shapeCommitDirFiles(
  rawFiles: ReadonlyArray<{
    filename: string;
    status?: string;
    additions?: number;
    deletions?: number;
    patch?: string;
    previous_filename?: string;
  }>,
  params: {
    filePage?: number;
    itemsPerPage?: number;
    charOffset?: number;
    charLength?: number;
  }
): {
  files: HistoryCommitFile[];
  filesPagination: {
    currentPage: number;
    totalPages: number;
    itemsPerPage: number;
    totalFiles: number;
    hasMore: boolean;
    nextFilePage?: number;
    terminalLimit?: boolean;
    continuationUnavailable?: {
      reason: 'schemaPageLimit';
      maxPage: number;
    };
  };
} {
  const allFiles: HistoryCommitFile[] = rawFiles.map(f => {
    const patchWindow =
      f.patch !== undefined
        ? windowPatch(f.patch, params.charOffset, params.charLength)
        : undefined;
    return {
      filename: f.filename,
      ...patchAvailability(f.patch),
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
      ...(patchWindow !== undefined
        ? {
            patch: patchWindow.patch,
            ...(patchWindow.patchPagination
              ? { patchPagination: patchWindow.patchPagination }
              : {}),
          }
        : {}),
      ...(f.previous_filename ? { previousFilename: f.previous_filename } : {}),
    } as HistoryCommitFile;
  });
  const filePage = Math.max(1, params.filePage ?? 1);
  const itemsPerPage = Math.max(1, params.itemsPerPage ?? 20);
  const totalFiles = allFiles.length;
  const totalPages = Math.max(1, Math.ceil(totalFiles / itemsPerPage));
  const currentPage = Math.min(filePage, totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  const files = allFiles.slice(start, start + itemsPerPage);
  return {
    files,
    filesPagination: {
      currentPage,
      totalPages,
      itemsPerPage,
      totalFiles,
      hasMore: currentPage < totalPages,
      ...(currentPage < totalPages && currentPage < MAX_PAGE_NUMBER
        ? { nextFilePage: currentPage + 1 }
        : {}),
      ...(currentPage < totalPages && currentPage >= MAX_PAGE_NUMBER
        ? {
            terminalLimit: true,
            continuationUnavailable: {
              reason: 'schemaPageLimit' as const,
              maxPage: MAX_PAGE_NUMBER,
            },
          }
        : {}),
    },
  };
}
