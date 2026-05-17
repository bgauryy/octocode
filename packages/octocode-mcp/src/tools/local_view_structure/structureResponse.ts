import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
import type { DirectoryEntry } from './structureFilters.js';
import type { WalkStats } from './structureWalker.js';

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
  query: { entriesPerPage?: number; entryPageNumber?: number }
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
    query.entriesPerPage || RESOURCE_LIMITS.DEFAULT_ENTRIES_PER_PAGE;
  const totalPages = Math.max(1, Math.ceil(totalEntries / entriesPerPage));
  const entryPageNumber = Math.min(query.entryPageNumber || 1, totalPages);
  const startIdx = (entryPageNumber - 1) * entriesPerPage;
  const endIdx = Math.min(startIdx + entriesPerPage, totalEntries);
  return {
    paginatedEntries: entries.slice(startIdx, endIdx),
    endIdx,
    pagination: {
      currentPage: entryPageNumber,
      totalPages,
      entriesPerPage,
      totalEntries,
      hasMore: entryPageNumber < totalPages,
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
  const hints = [
    `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${paginatedCount} of ${pagination.totalEntries})`,
  ];
  if (pagination.hasMore) {
    const nextPagePreview = entries
      .slice(endIdx, endIdx + 3)
      .map(e => e.name)
      .join(', ');
    hints.push(
      `Next: entryPageNumber=${pagination.currentPage + 1}${nextPagePreview ? ` (starts with: ${nextPagePreview}...)` : ''}`
    );
  } else {
    hints.push('Final page');
  }
  return hints;
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
