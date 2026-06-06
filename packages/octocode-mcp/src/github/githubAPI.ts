import type { components } from '@octokit/openapi-types';
import type { RestEndpointMethodTypes } from '@octokit/plugin-rest-endpoint-methods';

export type Repository = components['schemas']['full-repository'];

export type ContentDirectoryEntry =
  components['schemas']['content-directory'][number];

export type CodeSearchResultItem =
  components['schemas']['code-search-result-item'];

export type RepoSearchResultItem =
  components['schemas']['repo-search-result-item'];

export type IssueSearchResultItem =
  components['schemas']['issue-search-result-item'];

export type DiffEntry = components['schemas']['diff-entry'];

export type PullRequestItem = components['schemas']['pull-request'];

export type PullRequestSimple = components['schemas']['pull-request-simple'];

export type IssueComment = components['schemas']['issue-comment'];

export interface CommitFileInfo {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
  changes?: number;
  patch?: string;
}

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

  nonExistentScope?: boolean;
};

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

  archived?: boolean;
  sort?: 'created' | 'updated' | 'best-match';
  order?: 'asc' | 'desc';
  limit?: number;
  withComments?: boolean;
  withCommits?: boolean;

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
