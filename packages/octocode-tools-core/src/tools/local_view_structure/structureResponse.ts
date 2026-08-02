import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
import { LOCAL_DEFAULT_FILES_PER_PAGE } from '../../config.js';
import type { DirectoryEntry } from './structureFilters.js';

export interface WalkStats {
  skipped: number;
  permissionDenied: number;
  wasCapped?: boolean;
  rootError?: { code: string; message: string };
}

export function summarizeEntries(entries: DirectoryEntry[]): string {
  const totalFiles = entries.filter(e => e.type === 'file').length;
  const totalDirectories = entries.filter(e => e.type === 'directory').length;
  const totalSizeBytes = entries.reduce((sum, entry) => {
    return entry.type === 'file' && entry.size
      ? sum + parseFileSize(entry.size)
      : sum;
  }, 0);
  return `${entries.length} entries (${totalFiles} files, ${totalDirectories} dirs, ${formatFileSize(totalSizeBytes)})`;
}

export function paginateEntries(
  entries: DirectoryEntry[],
  query: { itemsPerPage?: number; page?: number }
): {
  paginatedEntries: DirectoryEntry[];
  endIdx: number;
  pagination: {
    currentPage: number;
    totalPages: number;
    entriesPerPage: number;
    totalEntries: number;
    hasMore: boolean;
    nextPage?: number;
    outOfRange?: boolean;
  };
} {
  const totalEntries = entries.length;
  const entriesPerPage = query.itemsPerPage || LOCAL_DEFAULT_FILES_PER_PAGE;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const requestedPage = query.page || 1;
  // Clamping to the last real page is kept (it's more useful than an empty
  // response) — but doing so SILENTLY lets a caller believe `currentPage`
  // (the clamped value) was what it actually asked for. `outOfRange` makes
  // the clamp visible instead of indistinguishable from a genuine last page.
  const currentPage = Math.min(requestedPage, totalPages);
  const isOutOfRange = requestedPage > totalPages;
  const startIdx = (currentPage - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
  const hasMore = currentPage < totalPages;
  return {
    paginatedEntries: entries.slice(startIdx, endIdx),
    endIdx,
    pagination: {
      currentPage,
      totalPages,
      entriesPerPage,
      totalEntries,
      hasMore,
      ...(hasMore ? { nextPage: currentPage + 1 } : {}),
      ...(isOutOfRange ? { outOfRange: true } : {}),
    },
  };
}

export function buildEntryPaginationHints(
  entries: DirectoryEntry[],
  paginatedCount: number,
  pagination: {
    currentPage: number;
    totalPages: number;
    totalEntries: number;
    hasMore: boolean;
  },
  endIdx: number
): string[] {
  if (!pagination.hasMore) return [];

  const nextPagePreview = entries
    .slice(endIdx, endIdx + 3)
    .map(e => e.name)
    .join(', ');
  return [
    `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${paginatedCount} of ${pagination.totalEntries}). Next: page=${pagination.currentPage + 1}${nextPagePreview ? ` (starts with: ${nextPagePreview}...)` : ''}`,
  ];
}

export function buildWalkWarnings(walkStats: WalkStats): string[] {
  if (walkStats.skipped <= 0) return [];
  const otherSkipped = walkStats.skipped - walkStats.permissionDenied;
  if (walkStats.permissionDenied > 0 && otherSkipped > 0) {
    return [
      `${walkStats.skipped} entries skipped (${walkStats.permissionDenied} permission denied, ${otherSkipped} other errors)`,
    ];
  }
  if (walkStats.permissionDenied > 0) {
    return [
      `${walkStats.permissionDenied} ${walkStats.permissionDenied === 1 ? 'entry' : 'entries'} skipped due to permission denied`,
    ];
  }
  return [
    `${walkStats.skipped} ${walkStats.skipped === 1 ? 'entry' : 'entries'} skipped due to access errors`,
  ];
}
