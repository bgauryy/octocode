/**
 * Provider Query Type Definitions
 * Unified query types for provider-agnostic code hosting operations.
 */

/**
 * Supported code hosting providers.
 * Default is 'github' .
 */
export type ProviderType = 'github' | 'gitlab' | 'bitbucket';

/**
 * Base query interface with common fields across all providers.
 */
export interface BaseProviderQuery {
  /** Provider to use (default: 'github') */
  provider?: ProviderType;
  /** Stable query identifier for matching input queries to response results */
  id?: string;
  /** Research context fields */
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

/**
 * Unified code search query parameters.
 */
export interface CodeSearchQuery extends BaseProviderQuery {
  /** Keywords to search for in code */
  keywords: string[];
  /**
   * Project identifier:
   * - GitHub: 'owner/repo'
   * - GitLab: numeric ID or 'group/project'
   */
  projectId?: string;
  /** Owner/organization filter (used when searching without a specific repo) */
  owner?: string;
  /** Filter by file path pattern */
  path?: string;
  /** Filter by filename */
  filename?: string;
  /** Filter by file extension (without dot) */
  extension?: string;
  /** Branch, tag, or commit reference */
  ref?: string;
  /** Search scope: 'file' for content, 'path' for filename/directory */
  match?: 'file' | 'path';
  /** Maximum results per page (max 100) */
  limit?: number;
  /** Page number for pagination */
  page?: number;
}

/**
 * Unified file content query parameters.
 */
export interface FileContentQuery extends BaseProviderQuery {
  /**
   * Project identifier:
   * - GitHub: 'owner/repo'
   * - GitLab: numeric ID or 'group/project'
   */
  projectId: string;
  /** File path within the repository */
  path: string;
  /** Branch, tag, or commit reference. Omit to use the provider default branch when supported. */
  ref?: string;
  /** Start line number for partial content */
  startLine?: number;
  /** End line number for partial content */
  endLine?: number;
  /** String to search for in file content */
  matchString?: string;
  /** Context lines around match */
  matchStringContextLines?: number;
  /** Character offset for byte-range fetching */
  charOffset?: number;
  /** Character length for byte-range fetching */
  charLength?: number;
  /** Whether to fetch full content */
  fullContent?: boolean;
}

/**
 * Unified repository search query parameters.
 */
export interface RepoSearchQuery extends BaseProviderQuery {
  /** Keywords to search for */
  keywords?: string[];
  /** Topics/tags to filter by */
  topics?: string[];
  /** Owner/organization filter */
  owner?: string;
  /** Minimum stars (GitHub only, GitLab: client-side filter) */
  minStars?: number;
  /** Raw stars filter string for GitHub (e.g. '>1000', '100..500', '>=500') */
  stars?: string;
  /** Size filter (e.g. '>1000', '100..500') */
  size?: string;
  /** Created date filter (e.g. '>2024-01-01') */
  created?: string;
  /** Updated/pushed date filter (e.g. '>2024-06-01') */
  updated?: string;
  /** Match scope: name, description, readme */
  match?: Array<'name' | 'description' | 'readme'>;
  /** Visibility filter */
  visibility?: 'public' | 'private' | 'internal';
  /** Sort by field */
  sort?: 'stars' | 'forks' | 'updated' | 'created' | 'best-match';
  /** Sort order */
  order?: 'asc' | 'desc';
  /** Maximum results per page */
  limit?: number;
  /** Page number */
  page?: number;
}

/**
 * Unified pull/merge request search query parameters.
 */
export interface PullRequestQuery extends BaseProviderQuery {
  /**
   * Project identifier (optional for cross-repo search):
   * - GitHub: 'owner/repo'
   * - GitLab: numeric ID or 'group/project'
   */
  projectId?: string;
  /** Owner/organization filter (used when searching without a specific repo) */
  owner?: string;
  /** Repository name filter */
  repo?: string;
  /**
   * PR/MR number within the project:
   * - GitHub: PR number
   * - GitLab: MR iid (project-scoped)
   */
  number?: number;
  /** State filter */
  state?: 'open' | 'closed' | 'merged' | 'all';
  /** Author username */
  author?: string;
  /** Assignee username */
  assignee?: string;
  /** Commenter username */
  commenter?: string;
  /** Involves username */
  involves?: string;
  /** Mentions username */
  mentions?: string;
  /** Review requested username */
  reviewRequested?: string;
  /** Reviewed by username */
  reviewedBy?: string;
  /** Label filter */
  labels?: string[];
  /** No label filter */
  noLabel?: boolean;
  /** No milestone filter */
  noMilestone?: boolean;
  /** No project filter */
  noProject?: boolean;
  /** No assignee filter */
  noAssignee?: boolean;
  /** Base branch filter */
  baseBranch?: string;
  /** Head branch filter */
  headBranch?: string;
  /** Created date filter (ISO 8601 or range like '>2024-01-01') */
  created?: string;
  /** Updated date filter */
  updated?: string;
  /** Closed date filter */
  closed?: string;
  /** Merged at date filter */
  mergedAt?: string;
  /** Comments count filter */
  comments?: number | string;
  /** Reactions count filter */
  reactions?: number | string;
  /** Interactions count filter */
  interactions?: number | string;
  /** Merged filter */
  merged?: boolean;
  /** Draft filter */
  draft?: boolean;
  /** Match scope: title, body, comments */
  match?: Array<'title' | 'body' | 'comments'>;
  /** Include PR comments */
  withComments?: boolean;
  /** Include commit details */
  withCommits?: boolean;
  /** Content type */
  type?: 'metadata' | 'fullContent' | 'partialContent';
  /** Partial content metadata for file filtering */
  partialContentMetadata?: {
    file: string;
    additions?: number[];
    deletions?: number[];
  }[];
  /** Sort field */
  sort?: 'created' | 'updated' | 'best-match';
  /** Sort order */
  order?: 'asc' | 'desc';
  /** Maximum results */
  limit?: number;
  /** Page number */
  page?: number;
}

/**
 * Unified repository structure query parameters.
 */
export interface RepoStructureQuery extends BaseProviderQuery {
  /**
   * Project identifier:
   * - GitHub: 'owner/repo'
   * - GitLab: numeric ID or 'group/project'
   */
  projectId: string;
  /** Branch, tag, or commit reference */
  ref?: string;
  /** Subdirectory path */
  path?: string;
  /** Maximum depth to traverse */
  depth?: number;
  /** Whether to fetch recursively */
  recursive?: boolean;
  /** Entries per page for pagination */
  entriesPerPage?: number;
  /** Page number for entries */
  entryPageNumber?: number;
}
