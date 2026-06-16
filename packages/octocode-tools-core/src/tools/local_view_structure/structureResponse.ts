import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
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
  };
} {
  const totalEntries = entries.length;
  const entriesPerPage =
    query.itemsPerPage || RESOURCE_LIMITS.DEFAULT_ENTRIES_PER_PAGE;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const currentPage = Math.min(query.page || 1, totalPages);
  const startIdx = (currentPage - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
  return {
    paginatedEntries: entries.slice(startIdx, endIdx),
    endIdx,
    pagination: {
      currentPage,
      totalPages,
      entriesPerPage,
      totalEntries,
      hasMore: currentPage < totalPages,
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
