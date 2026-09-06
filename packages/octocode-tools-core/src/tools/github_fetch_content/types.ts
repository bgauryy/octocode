import type { z } from 'zod';
import type { FileContentQuerySchema } from '../../toolContract/input/resources/tools/ghGetFileContent.js';
import type { MinifyMode } from '../../scheme/fields.js';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { PaginationInfo } from '../../types/toolResults.js';

export type FileContentExecutionQuery = FileContentQuery & {
  noTimestamp?: boolean;
  minify: MinifyMode;
  contextLines?: number;
  matchStringIsRegex?: boolean;
  matchStringCaseSensitive?: boolean;
  charOffset?: number;
  charLength?: number;
};

export interface GitHubFileContentApiData {
  owner?: string;
  repo?: string;
  path?: string;
  content?: string;
  contentView?: 'none' | 'standard' | 'symbols';
  branch?: string;
  resolvedBranch?: string;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  errorCode?: 'contentSecurityLimit';
  terminalLimit?: boolean;
  partialReasons?: Array<'security-selected-view-size-limit'>;
  totalLines?: number;
  sourceChars?: number;

  matchRanges?: Array<{ start: number; end: number }>;
  /** Exact matched-line numbers (matchRanges are ±contextLines windows around them). */
  matchedLines?: number[];
  matchLocations?: string[];
  warnings?: string[];
  lastModified?: string;
  lastModifiedBy?: string;
  pagination?: PaginationInfo;
  cached?: boolean;
  matchNotFound?: boolean;
  searchedFor?: string;
}

interface GitHubFileContentApiResultBase {
  error?: string;
  hints?: string[];
}

export interface GitHubFileContentApiResult
  extends GitHubFileContentApiResultBase, GitHubFileContentApiData {}

export interface DirectoryFetchResult {
  localPath: string;
  repoRoot: string;
  files: Array<{ path: string; size: number; type: string }>;
  fileCount: number;
  totalSize: number;
  /** true = no files were skipped by size/type/limit/error */
  complete: boolean;
  /** true = completeness was proven against the remote tree (fresh fetch + complete) */
  verified: boolean;
  /** Immutable commit identity used by repoRoot and localPath. */
  commitSha: string;
  /** true when nonFile > 0 — subdirectory entries were present but not fetched; use ghCloneRepo for full coverage */
  hasSubdirectories?: boolean;
  directoryEntryCount: number;
  eligibleFileCount: number;
  savedFileCount: number;
  skipped: {
    nonFile: number;
    oversized: number;
    binary: number;
    fileLimit: number;
    fetchFailed: number;
    totalSizeLimit: number;
    pathTraversal: number;
  };
  limits: {
    maxDirectoryFiles: number;
    maxTotalSize: number;
    maxFileSize: number;
  };
  warnings?: string[];
  cached: boolean;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
  directoryPath: string;
}

export interface FileMaterializationResult {
  localPath: string;
  repoRoot: string;
  path: string;
  size: number;
  cached: boolean;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
  commitSha: string;
}
