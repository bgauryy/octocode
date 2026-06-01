/**
 * Provider Result Type Definitions
 * Unified result types for provider-agnostic code hosting operations.
 */

import type { PaginationInfo } from '../types/toolResults.js';

/**
 * Unified repository information.
 */
export interface UnifiedRepository {
  /** Unique identifier (provider-specific format) */
  id: string;
  /** Repository name */
  name: string;
  /** Full path (owner/repo or group/project) */
  fullPath: string;
  /** Description */
  description: string | null;
  /** Web URL */
  url: string;
  /** Clone URL (HTTPS) */
  cloneUrl: string;
  /** Default branch */
  defaultBranch: string;
  /** Star count */
  stars: number;
  /** Fork count */
  forks: number;
  /** Visibility */
  visibility: 'public' | 'private' | 'internal';
  /** Topics/tags */
  topics: string[];
  /** Created date */
  createdAt: string;
  /** Updated date */
  updatedAt: string;
  /** Last activity date */
  lastActivityAt: string;
  /** Open issues count */
  openIssuesCount?: number;
  /** Archived status */
  archived?: boolean;
  /** Primary programming language */
  language?: string;
}

/**
 * Unified code search result item.
 */
export interface CodeSearchItem {
  /** File path */
  path: string;
  /** Match context/content */
  matches: Array<{
    context: string;
    positions: [number, number][];
  }>;
  /** File URL */
  url: string;
  /** Repository info */
  repository: {
    id: string;
    name: string;
    url: string;
  };
  /** Last modified date */
  lastModifiedAt?: string;
}

/**
 * Unified code search result.
 */
export interface CodeSearchResult {
  /** Search result items */
  items: CodeSearchItem[];
  /** Total count */
  totalCount: number;
  /** Pagination info */
  pagination: PaginationInfo;
  /** Repository context (if single repo search) */
  repositoryContext?: {
    owner: string;
    repo: string;
    branch?: string;
  };
  /**
   * True when the empty result is a nonexistent searched owner/repo/user
   * (GitHub 422), not a valid scope that matched nothing.
   */
  nonExistentScope?: boolean;
}

/**
 * Unified file content result.
 */
export interface FileContentResult {
  /** File path */
  path: string;
  /** File content */
  content: string;
  /** Content encoding */
  encoding: 'utf-8' | 'base64';
  /** File size in bytes */
  size: number;
  /** Total number of lines in the source file, when known */
  totalLines?: number;
  /** Branch/ref used */
  ref: string;
  /** Last modified date */
  lastModified?: string;
  /** Last modified by */
  lastModifiedBy?: string;
  /** Last commit SHA */
  lastCommitSha?: string;
  /** Pagination for partial content */
  pagination?: PaginationInfo;
  /** Whether content is partial */
  isPartial?: boolean;
  /** Start line (if partial) */
  startLine?: number;
  /** End line (if partial) */
  endLine?: number;
  /** Non-fatal warnings/notices from extraction or sanitization */
  warnings?: string[];
}

/**
 * Unified repository search result.
 */
export interface RepoSearchResult {
  /** Found repositories */
  repositories: UnifiedRepository[];
  /** Total count */
  totalCount: number;
  /** Pagination info */
  pagination: PaginationInfo;
  /**
   * True when the empty result is a nonexistent searched owner/user (GitHub
   * 422), not a valid scope that matched nothing.
   */
  nonExistentScope?: boolean;
}

/**
 * Unified pull/merge request item.
 */
export interface PullRequestItem {
  /** PR/MR number */
  number: number;
  /** Title */
  title: string;
  /** Description/body */
  body: string | null;
  /** Web URL */
  url: string;
  /** State */
  state: 'open' | 'closed' | 'merged';
  /** Draft status */
  draft: boolean;
  /** Author username */
  author: string;
  /** Assignees */
  assignees: string[];
  /** Labels */
  labels: string[];
  /** Source branch */
  sourceBranch: string;
  /** Target branch */
  targetBranch: string;
  /** Source SHA */
  sourceSha?: string;
  /** Target SHA */
  targetSha?: string;
  /** Created date */
  createdAt: string;
  /** Updated date */
  updatedAt: string;
  /** Closed date */
  closedAt?: string;
  /** Merged date */
  mergedAt?: string;
  /** Comment count */
  commentsCount?: number;
  /** Changed files count */
  changedFilesCount?: number;
  /** Additions count */
  additions?: number;
  /** Deletions count */
  deletions?: number;
  /** Comments (if requested) */
  comments?: Array<{
    id: string;
    author: string;
    body: string;
    createdAt: string;
    updatedAt: string;
  }>;
  /** File changes (if requested) */
  fileChanges?: Array<{
    path: string;
    status: string;
    additions: number;
    deletions: number;
    patch?: string;
  }>;
}

/**
 * Unified pull/merge request search result.
 */
export interface PullRequestSearchResult {
  /** Found PRs/MRs */
  items: PullRequestItem[];
  /** Total count */
  totalCount: number;
  /** Pagination info */
  pagination: PaginationInfo;
  /** Repository context (if single repo) */
  repositoryContext?: {
    owner: string;
    repo: string;
  };
}

/**
 * Directory entry in repository structure.
 */
export interface DirectoryEntry {
  files: string[];
  folders: string[];
}

/**
 * Unified repository structure result.
 */
export interface RepoStructureResult {
  /** Project path */
  projectPath: string;
  /** Branch/ref that was actually used */
  branch: string;
  /** Default branch of the repository (populated when a branch fallback occurred) */
  defaultBranch?: string;
  /** Current path */
  path: string;
  /** Structure by directory */
  structure: Record<string, DirectoryEntry>;
  /** Summary */
  summary: {
    totalFiles: number;
    totalFolders: number;
    truncated: boolean;
  };
  /** Pagination info */
  pagination?: PaginationInfo;
  /** Hints for user */
  hints?: string[];
}
