import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { MinifyMode } from '../../scheme/fields.js';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { PaginationInfo } from '../../utils/core/types.js';

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
  isSkeleton?: boolean;
  branch?: string;
  resolvedBranch?: string;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;

  matchRanges?: Array<{ start: number; end: number }>;
  matchLocations?: string[];
  warnings?: string[];
  lastModified?: string;
  lastModifiedBy?: string;
  pagination?: PaginationInfo;
  cached?: boolean;
  matchNotFound?: boolean;
  searchedFor?: string;

  signaturesExtracted?: boolean;
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
