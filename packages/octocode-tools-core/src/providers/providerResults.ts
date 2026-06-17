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

  sourceChars?: number;

  sourceBytes?: number;

  contentView?: 'none' | 'standard' | 'symbols';

  isSkeleton?: boolean;

  ref: string;

  lastModified?: string;

  lastModifiedBy?: string;

  lastCommitSha?: string;

  pagination?: PaginationInfo;

  isPartial?: boolean;

  startLine?: number;

  endLine?: number;

  matchRanges?: Array<{ start: number; end: number }>;

  warnings?: string[];

  matchNotFound?: boolean;

  searchedFor?: string;
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

  bodyPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    nextCharOffset?: number;
  };

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
    bodyPagination?: {
      charOffset: number;
      charLength: number;
      totalChars: number;
      hasMore: boolean;
      nextCharOffset?: number;
    };

    commentType?: 'discussion' | 'review_inline';

    path?: string;

    line?: number;

    in_reply_to_id?: number;
  }>;

  reviews?: Array<{
    id: string;
    user: string;
    state: string;
    body: string;
    submittedAt?: string;
    commitId?: string;
  }>;

  commits?: Array<{
    sha: string;
    message: string;
    author: string;
    date: string;
  }>;

  fileChanges?: Array<{
    path: string;
    status: string;
    additions: number;
    deletions: number;
    patch?: string;
  }>;

  sanitizationWarnings?: string[];
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

  fileSizeMap?: Record<string, Record<string, number>>;

  summary: {
    totalFiles: number;
    totalFolders: number;
    truncated: boolean;
  };

  pagination?: PaginationInfo;

  hints?: string[];
}
