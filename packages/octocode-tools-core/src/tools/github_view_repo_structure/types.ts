import type { GitHubRepoStructureDirectoryEntry } from '@octocodeai/octocode-core/extra-types';
import type { ContentDirectoryEntry } from '../../github/githubAPI.js';
import type { PaginationInfo } from '../../types/toolResults.js';

export type GitHubApiFileItem = ContentDirectoryEntry;

export interface GitHubRepositoryStructureResult {
  owner: string;
  repo: string;
  branch: string;
  defaultBranch?: string;
  path: string;
  apiSource: boolean;
  summary: {
    totalFiles: number;
    totalFolders: number;
    truncated: boolean;
    filtered: boolean;
    originalCount: number;
  };
  structure: Record<string, GitHubRepoStructureDirectoryEntry>;
  fileSizeMap?: Record<string, Record<string, number>>;
  pagination?: PaginationInfo;
  hints?: string[];
  _cachedItems?: { path: string; type: 'file' | 'dir' }[];
  _cachedFileSizeMap?: Record<string, Record<string, number>>;
  rawResponseChars?: number;
}

export interface GitHubRepositoryStructureError {
  error: string;
  status?: number;
  triedBranches?: string[];
  defaultBranch?: string;
  rateLimitRemaining?: number;
  rateLimitReset?: number;
  retryAfter?: number;
  hints?: string[];
}
