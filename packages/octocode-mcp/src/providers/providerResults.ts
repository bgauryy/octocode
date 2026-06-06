import type { PaginationInfo } from '../types/toolResults.js';

export interface UnifiedRepository {
  id: string;

  name: string;

  fullPath: string;

  description: string | null;

  url: string;

  cloneUrl: string;

  defaultBranch: string;

  stars: number;

  forks: number;

  visibility: 'public' | 'private' | 'internal';

  topics: string[];

  createdAt: string;

  updatedAt: string;

  lastActivityAt: string;

  openIssuesCount?: number;

  archived?: boolean;

  language?: string;
}

export interface CodeSearchItem {
  path: string;

  matches: Array<{
    context: string;
    positions: [number, number][];
  }>;

  url: string;

  repository: {
    id: string;
    name: string;
    url: string;
  };

  lastModifiedAt?: string;
}

export interface CodeSearchResult {
  items: CodeSearchItem[];

  totalCount: number;

  pagination: PaginationInfo;

  repositoryContext?: {
    owner: string;
    repo: string;
    branch?: string;
  };

  nonExistentScope?: boolean;
}

export interface FileContentResult {
  path: string;

  content: string;

  encoding: 'utf-8' | 'base64';

  size: number;

  totalLines?: number;

  ref: string;

  lastModified?: string;

  lastModifiedBy?: string;

  lastCommitSha?: string;

  pagination?: PaginationInfo;

  isPartial?: boolean;

  startLine?: number;

  endLine?: number;

  warnings?: string[];
}

export interface RepoSearchResult {
  repositories: UnifiedRepository[];

  totalCount: number;

  pagination: PaginationInfo;

  nonExistentScope?: boolean;
}

export interface PullRequestItem {
  number: number;

  title: string;

  body: string | null;

  url: string;

  state: 'open' | 'closed' | 'merged';

  draft: boolean;

  author: string;

  assignees: string[];

  labels: string[];

  sourceBranch: string;

  targetBranch: string;

  sourceSha?: string;

  targetSha?: string;

  createdAt: string;

  updatedAt: string;

  closedAt?: string;

  mergedAt?: string;

  commentsCount?: number;

  changedFilesCount?: number;

  additions?: number;

  deletions?: number;

  comments?: Array<{
    id: string;
    author: string;
    body: string;
    createdAt: string;
    updatedAt: string;
  }>;

  fileChanges?: Array<{
    path: string;
    status: string;
    additions: number;
    deletions: number;
    patch?: string;
  }>;
}

export interface PullRequestSearchResult {
  items: PullRequestItem[];

  totalCount: number;

  pagination: PaginationInfo;

  repositoryContext?: {
    owner: string;
    repo: string;
  };
}

export interface DirectoryEntry {
  files: string[];
  folders: string[];
}

export interface RepoStructureResult {
  projectPath: string;

  branch: string;

  defaultBranch?: string;

  path: string;

  structure: Record<string, DirectoryEntry>;

  summary: {
    totalFiles: number;
    totalFolders: number;
    truncated: boolean;
  };

  pagination?: PaginationInfo;

  hints?: string[];
}
