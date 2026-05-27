import type { FileContentQuery } from '@octocodeai/octocode-core';
import type { PaginationInfo } from '../../utils/core/types.js';

/**
 * Extension of FileContentQuery with internal execution fields
 * not part of the Zod schema.
 */
export type FileContentExecutionQuery = FileContentQuery & {
  noTimestamp?: boolean;
};

export interface GitHubFileContentApiData {
  owner?: string;
  repo?: string;
  path?: string;
  content?: string;
  branch?: string;
  resolvedBranch?: string;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  totalLines?: number;
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
  files: Array<{ path: string; size: number; type: string }>;
  fileCount: number;
  totalSize: number;
  cached: boolean;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
  directoryPath: string;
}
