import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { PaginationInfo } from '../../../types/toolResults.js';
import type { QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { GitHubFetchContentOutputLocal } from '../scheme.js';
import type { WithOptionalMeta } from '../../../types/execution.js';

export type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

export type FileEntry = {
  path: string;
  content: string;
  localPath?: string;
  repoRoot?: string;
  fileSize?: number;
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  resolvedBranch?: string;
  pagination?: PaginationInfo;
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
  cached?: boolean;
  next?: FileContentNextMap;
};

export type FileContentNextMap = {
  continueChars?: {
    tool: 'ghGetFileContent';
    query: Record<string, unknown>;
  };
  cloneForSemantics?: {
    tool: 'ghCloneRepo';
    query: Record<string, unknown>;
    why: string;
    confidence: 'exact';
  };
};

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
    missingDownloadUrl: number;
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
};

export type RepoGroup = {
  id: string;
  owner: string;
  repo: string;
  files?: FileEntry[];
  directories?: DirectoryEntry[];
  data?: RepoGroupData;
};

export type RepoGroupData = {
  owner: string;
  repo: string;
  files?: FileEntry[];
  directories?: DirectoryEntry[];
};

export type FileContentResponse = GitHubFetchContentOutputLocal;
