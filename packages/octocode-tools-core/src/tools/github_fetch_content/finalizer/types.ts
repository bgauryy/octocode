import type { z } from 'zod';
import type { FileContentQuerySchema } from '../../../toolContract/input/resources/tools/ghGetFileContent.js';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { PaginationInfo } from '../../../types/toolResults.js';
import type { QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { WithOptionalMeta } from '../../../types/execution.js';

export type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

export type FileEntry = {
  path: string;
  content: string;
  /** Coarse file bucket to guide how bytes are read; omitted when uncertain. */
  fileType?: 'code' | 'config' | 'lock' | 'doc';
  localPath?: string;
  repoRoot?: string;
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  resolvedBranch?: string;
  commitSha?: string;
  pagination?: PaginationInfo;
  isPartial?: boolean;
  errorCode?: 'contentSecurityLimit';
  terminalLimit?: boolean;
  partialReasons?: Array<'security-selected-view-size-limit'>;
  startLine?: number;
  endLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  /** Exact matched-line numbers (matchRanges are ±contextLines windows around them). */
  matchedLines?: number[];
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
  cached?: boolean;
  next?: FileContentNextMap;
};

export type FileContentNextMap = {
  readBoundedLines?: {
    tool: 'ghGetFileContent';
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact';
  };
  continueChars?: {
    tool: 'ghGetFileContent';
    query: Record<string, unknown>;
  };
  continueLines?: {
    tool: 'ghGetFileContent';
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact';
  };
  cloneForSemantics?: {
    tool: 'ghCloneRepo';
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact';
  };
  escalateToClone?: {
    tool: 'ghCloneRepo';
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact';
  };
};

export type DirectoryPartialReason =
  keyof NonNullable<DirectoryEntry['skipped']> | 'providerDirectoryIncomplete';

export type DirectoryEntry = {
  path: string;
  localPath: string;
  repoRoot?: string;
  fileCount: number;
  totalSize: number;
  complete?: boolean;
  verified?: boolean;
  commitSha?: string;
  hasSubdirectories?: boolean;
  skippedSummary?: Record<string, number>;
  directoryEntryCount?: number;
  eligibleFileCount?: number;
  savedFileCount?: number;
  skipped?: {
    nonFile: number;
    oversized: number;
    binary: number;
    fileLimit: number;
    fetchFailed: number;
    totalSizeLimit: number;
    pathTraversal: number;
  };
  limits?: {
    maxDirectoryFiles: number;
    maxTotalSize: number;
    maxFileSize: number;
  };
  warnings?: string[];
  files?: Array<{ path: string; size: number; type: string }>;
  cached?: boolean;
  resolvedBranch?: string;
  isPartial?: boolean;
  terminalLimit?: boolean;
  partialReasons?: DirectoryPartialReason[];
  next?: Pick<FileContentNextMap, 'escalateToClone'>;
};
