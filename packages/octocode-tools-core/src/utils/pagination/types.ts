export interface PaginationMetadata {
  paginatedContent: string;

  byteOffset: number;

  byteLength: number;

  totalBytes: number;

  nextByteOffset?: number;

  charOffset: number;

  charLength: number;

  totalChars: number;

  nextCharOffset?: number;

  hasMore: boolean;
  estimatedTokens?: number;
  currentPage: number;
  totalPages: number;
}

export interface ApplyPaginationOptions {
  actualOffset?: number;

  mode?: 'characters' | 'bytes';

  // Stable page size for the currentPage/totalPages math. When pages are
  // snapped to semantic boundaries, the per-page slice length varies, which
  // makes `Math.floor(offset / sliceLength) + 1` report a relative counter
  // (e.g. "Page 1/21" on a continuation). Passing the original requested page
  // size keeps the page counter absolute and monotonic. Falls back to `length`.
  pageSize?: number;
}

export interface GeneratePaginationHintsOptions {
  enableWarnings?: boolean;
  customHints?: string[];
  toolName?: string;
}

export interface StructurePaginationInfo {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage: number;
  totalEntries: number;
}

export interface StructurePaginationHintContext {
  owner: string;
  repo: string;
  branch: string;
  path?: string;
  depth?: number;
  pageFiles: number;
  pageFolders: number;
  allFiles: number;
  allFolders: number;
}
