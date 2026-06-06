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
}

export interface GeneratePaginationHintsOptions {
  enableWarnings?: boolean;
  customHints?: string[];
  toolName?: string;
}

export interface GitHubFileContentHintContext {
  owner: string;
  repo: string;
  path: string;
  branch?: string;
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

export interface SliceByCharResult {
  sliced: string;
  actualOffset: number;
  actualLength: number;
  hasMore: boolean;
  nextOffset?: number;
  lineCount: number;
  totalChars: number;
}
