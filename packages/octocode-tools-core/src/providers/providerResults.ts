import type { CollectionStates } from '../github/prContentFetcher/collectionPaging.js';
import type { PaginationInfo } from '../types/toolResults.js';
import type { PRProviderLimit } from '../github/githubAPI.js';

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
  license?: string;
  homepage?: string;
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

  /** Provider's search index did not fully complete (GitHub incomplete_results) — empty/partial results may be a false negative. */
  incompleteResults?: boolean;
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

  errorCode?: 'contentSecurityLimit';
  terminalLimit?: boolean;
  partialReasons?: Array<'security-selected-view-size-limit'>;

  ref: string;

  lastModified?: string;

  lastModifiedBy?: string;

  lastCommitSha?: string;

  pagination?: PaginationInfo;

  isPartial?: boolean;

  startLine?: number;

  endLine?: number;

  matchRanges?: Array<{ start: number; end: number }>;

  /** Exact matched-line numbers (matchRanges are ±contextLines windows around them). */
  matchedLines?: number[];

  warnings?: string[];

  matchNotFound?: boolean;

  searchedFor?: string;
}

export interface RepoSearchResult {
  repositories: UnifiedRepository[];
  incompleteResults?: boolean;

  totalCount: number;

  pagination: PaginationInfo;

  nonExistentScope?: boolean;
}

export interface PullRequestItem {
  collectionStates?: CollectionStates;
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
    filesCollectionState?: import('../github/prContentFetcher/collectionPaging.js').CollectionState;
    sha: string;
    message: string;
    author: string;
    date: string;
    files?: Array<{
      filename: string;
      status: string;
      additions: number;
      deletions: number;
      changes?: number;
      patch?: string;
    }>;
  }>;

  providerLimits?: PRProviderLimit[];

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

  incompleteResults?: boolean;

  pagination: PaginationInfo;

  /**
   * The exact search-qualifier string sent to the provider's search API
   * (e.g. `"useTransition" is:pr repo:facebook/react`). Present only when
   * the search path ran — its absence tells the caller a plain listing was
   * served instead of a keyword search.
   */
  effectiveQuery?: string;

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
    incompleteTree?: boolean;
  };

  pagination?: PaginationInfo;

  hints?: string[];

  isPartial?: boolean;

  terminalLimit?: boolean;

  partialReasons?: Array<
    | 'providerTreeTruncated'
    | 'partialTreeFailures'
    | 'metadataPagination'
    | 'metadataFetchFailed'
    | 'metadataPageLimit'
  >;

  metadataPagination?: Partial<
    Record<
      'contributors' | 'branches' | 'tags' | 'languages',
      {
        currentPage: number;
        perPage: number;
        returned: number;
        hasMore: boolean;
        failed?: boolean;
        terminalLimit?: boolean;
      }
    >
  >;
}
