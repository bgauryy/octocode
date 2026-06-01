import type { components } from '@octokit/openapi-types';
import type { RestEndpointMethodTypes } from '@octokit/plugin-rest-endpoint-methods';

/**
 * Octokit REST types from OpenAPI-generated schemas.
 * @see https://github.com/octokit/openapi-types
 * @see https://github.com/github/rest-api-description
 */

/** Full repository details. Schema: components['schemas']['full-repository'] */
export type Repository = components['schemas']['full-repository'];

/**
 * Content directory entry (single item from directory listing).
 * Schema: components['schemas']['content-directory'][number]
 *
 * Note: The 'content-directory' schema is an array type, so we index into it
 * to get the type of a single entry.
 */
export type ContentDirectoryEntry =
  components['schemas']['content-directory'][number];

/** Code search result item. Schema: components['schemas']['code-search-result-item'] */
export type CodeSearchResultItem =
  components['schemas']['code-search-result-item'];

/** Repository search result item. Schema: components['schemas']['repo-search-result-item'] */
export type RepoSearchResultItem =
  components['schemas']['repo-search-result-item'];

/** Issue/PR search result item. Schema: components['schemas']['issue-search-result-item'] */
export type IssueSearchResultItem =
  components['schemas']['issue-search-result-item'];

/** Diff entry for file changes. Schema: components['schemas']['diff-entry'] */
export type DiffEntry = components['schemas']['diff-entry'];

/** Full pull request details. Schema: components['schemas']['pull-request'] */
export type PullRequestItem = components['schemas']['pull-request'];

/** Simplified pull request. Schema: components['schemas']['pull-request-simple'] */
export type PullRequestSimple = components['schemas']['pull-request-simple'];

/** Issue comment. Schema: components['schemas']['issue-comment'] */
export type IssueComment = components['schemas']['issue-comment'];

/** Commit file change information (merged from CommitFileItem) */
export interface CommitFileInfo {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
  changes?: number;
  patch?: string;
}

/** Commit information with file changes */
export interface CommitInfo {
  sha: string;
  message: string;
  author: string;
  date: string;
  files: CommitFileInfo[];
}

export type GetContentParameters =
  RestEndpointMethodTypes['repos']['getContent']['parameters'];
export type SearchCodeParameters =
  RestEndpointMethodTypes['search']['code']['parameters'];
export type SearchCodeResponse =
  RestEndpointMethodTypes['search']['code']['response'];
export type SearchReposParameters =
  RestEndpointMethodTypes['search']['repos']['parameters'];

export interface GitHubAPIError {
  error: string;
  status?: number;
  type: 'http' | 'graphql' | 'network' | 'unknown';
  scopesSuggestion?: string;
  rateLimitRemaining?: number;
  /** Rate limit reset time in **milliseconds** (from `resetTime.getTime()`). */
  rateLimitReset?: number;
  retryAfter?: number;
  hints?: string[];
}

export interface GitHubAPISuccess<T> {
  data: T;
  status: number;
  headers?: Record<string, string>;
  rawResponseChars?: number;
}

export type GitHubAPIResponse<T> = GitHubAPISuccess<T> | GitHubAPIError;

/**
 * Optimized code search result with enhanced match information.
 *
 * Derives from CodeSearchResultItem (components['schemas']['code-search-result-item'])
 * but with optimized structure for our search operations.
 */
export type OptimizedCodeSearchResult = {
  items: Array<
    Pick<CodeSearchResultItem, 'path' | 'url'> & {
      matches: Array<{
        context: string;
        positions: [number, number][];
      }>;
      repository: {
        nameWithOwner: string;
        url: string;
        pushedAt?: string;
      };
      minificationType?: string;
      lastModifiedAt?: string;
    }
  >;
  total_count: number;
  repository?: {
    name: string;
    url: string;
    createdAt?: string;
    updatedAt?: string;
    pushedAt?: string;
  };
  matchLocations?: string[];
  minified?: boolean;
  minificationFailed?: boolean;
  minificationTypes?: string[];
  _researchContext?: {
    foundFiles: string[];
    repositoryContext?: {
      owner: string;
      repo: string;
      branch?: string;
    };
  };
  pagination?: {
    currentPage: number;
    totalPages: number;
    perPage: number;
    totalMatches: number;
    hasMore: boolean;
  };
  /**
   * True when the empty result is because GitHub reported the searched
   * owner/repo/user does NOT exist (a 422 nonexistent-entity), as opposed to a
   * valid scope that genuinely matched nothing. Lets the caller emit a
   * "check the scope spelling" hint instead of "no matches found".
   */
  nonExistentScope?: boolean;
};

/**
 * Custom pull request item for search results.
 *
 * This type picks common fields from IssueSearchResultItem (which GitHub uses for
 * both issues and PRs in search results) and extends with PR-specific fields.
 *
 * Related Octokit types:
 * - IssueSearchResultItem: components['schemas']['issue-search-result-item']
 * - PullRequestItem: components['schemas']['pull-request'] (full PR details)
 * - DiffEntry: components['schemas']['diff-entry'] (file changes)
 */
/**
 * Comment structure from GitHub REST API (issues.listComments).
 * Note: This uses snake_case to match REST API conventions.
 */
export interface PRCommentItem {
  id: string;
  user: string;
  body: string;
  created_at: string;
  updated_at: string;
}

export type GitHubPullRequestItem = Pick<
  IssueSearchResultItem,
  | 'number'
  | 'title'
  | 'body'
  | 'state'
  | 'created_at'
  | 'updated_at'
  | 'closed_at'
  | 'url'
  | 'draft'
> & {
  author: string;
  labels: string[];
  merged_at?: string;
  comments?: PRCommentItem[];
  reactions: number;
  head?: string;
  base?: string;
  head_sha?: string;
  base_sha?: string;
  file_changes?: {
    total_count: number;
    files: DiffEntry[];
  };
  commits?: CommitInfo[];
  _sanitization_warnings?: string[];
};

export interface GitHubPullRequestsSearchParams {
  query?: string;
  owner?: string | string[];
  repo?: string | string[];
  prNumber?: number;
  state?: 'open' | 'closed';
  draft?: boolean;
  merged?: boolean;
  author?: string;
  assignee?: string;
  mentions?: string;
  commenter?: string;
  involves?: string;
  'reviewed-by'?: string;
  'review-requested'?: string;
  head?: string;
  base?: string;
  created?: string;
  updated?: string;
  'merged-at'?: string;
  closed?: string;
  comments?: number | string;
  reactions?: number | string;
  interactions?: number | string;
  label?: string | string[];
  'no-assignee'?: boolean;
  'no-label'?: boolean;
  'no-milestone'?: boolean;
  'no-project'?: boolean;
  match?: ('title' | 'body' | 'comments')[];
  /** Include PRs from archived repositories. Default false excludes them. */
  archived?: boolean;
  sort?: 'created' | 'updated' | 'best-match';
  order?: 'asc' | 'desc';
  limit?: number;
  withComments?: boolean;
  withCommits?: boolean;
  /**
   * When fetching comments, include bot authors (vercel[bot], CodeRabbit, …).
   * Default false — bot deploy tables / base64 status blobs are stripped to
   * keep the review signal cheap. Set true to restore the full thread.
   */
  includeBots?: boolean;
  type?: 'metadata' | 'fullContent' | 'partialContent';
  partialContentMetadata?: {
    file: string;
    additions?: number[];
    deletions?: number[];
  }[];
  exhaustive?: boolean;
  maxPages?: number;
  pageSize?: number;
  page?: number;
}

export function isGitHubAPIError(obj: unknown): obj is GitHubAPIError {
  return !!(
    obj &&
    typeof obj === 'object' &&
    obj !== null &&
    'error' in obj &&
    typeof (obj as Record<string, unknown>).error === 'string' &&
    'type' in obj
  );
}

export function isGitHubAPISuccess<T>(
  obj: unknown
): obj is GitHubAPISuccess<T> {
  return !!(
    obj &&
    typeof obj === 'object' &&
    obj !== null &&
    'data' in obj &&
    'status' in obj &&
    typeof (obj as Record<string, unknown>).status === 'number'
  );
}

export function isRepository(obj: unknown): obj is Repository {
  return !!(
    obj &&
    typeof obj === 'object' &&
    obj !== null &&
    'id' in obj &&
    typeof (obj as Record<string, unknown>).id === 'number' &&
    'name' in obj &&
    typeof (obj as Record<string, unknown>).name === 'string' &&
    'full_name' in obj &&
    typeof (obj as Record<string, unknown>).full_name === 'string' &&
    'private' in obj &&
    typeof (obj as Record<string, unknown>).private === 'boolean'
  );
}
