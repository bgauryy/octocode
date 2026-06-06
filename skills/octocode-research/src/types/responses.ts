import { hasProperty, hasArrayProperty } from './guards.js';


export interface FileMatch {
  path: string;
  line?: number;
  column?: number;
  matchText?: string;
  matchCount?: number;
  contextBefore?: string[];
  contextAfter?: string[];
  allMatches?: Array<{
    line: number;
    column?: number;
    value?: string;
    byteOffset?: number;
    charOffset?: number;
  }>;
}

export interface PaginationInfo {
  page?: number;
  currentPage?: number;
  totalPages?: number;
  totalMatches?: number;
  totalFiles?: number;
  hasMore?: boolean;
  nextCursor?: string;
}


function isFileMatch(obj: unknown): obj is FileMatch {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'path' in obj &&
    typeof (obj as FileMatch).path === 'string'
  );
}

function hasValidPagination(obj: unknown): obj is { pagination: PaginationInfo } {
  if (!hasProperty(obj, 'pagination')) return false;
  const p = obj.pagination;
  return typeof p === 'object' && p !== null;
}


export function extractFiles(data: unknown): FileMatch[] {
  if (!hasArrayProperty(data, 'files')) return [];
  return (data as { files: unknown[] }).files.filter(isFileMatch);
}

export function extractPagination(data: unknown): PaginationInfo | undefined {
  if (!hasValidPagination(data)) return undefined;
  return data.pagination;
}

export function extractTotalMatches(data: unknown): number {
  if (hasProperty(data, 'totalMatches') && typeof data.totalMatches === 'number') {
    return data.totalMatches;
  }
  if (hasValidPagination(data) && typeof data.pagination.totalMatches === 'number') {
    return data.pagination.totalMatches;
  }
  return 0;
}
