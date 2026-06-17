type ProviderType = 'github';

interface BaseProviderQuery {
  provider?: ProviderType;

  id?: string;

  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

export interface CodeSearchQuery extends BaseProviderQuery {
  keywords: string[];

  projectId?: string;

  owner?: string;

  path?: string;

  filename?: string;

  extension?: string;

  language?: string;

  match?: 'file' | 'path';

  limit?: number;

  page?: number;
}

export interface FileContentQuery extends BaseProviderQuery {
  projectId: string;

  path: string;

  ref?: string;

  startLine?: number;

  endLine?: number;

  matchString?: string;

  contextLines?: number;

  matchStringIsRegex?: boolean;

  matchStringCaseSensitive?: boolean;

  charOffset?: number;

  charLength?: number;

  fullContent?: boolean;

  minify: 'none' | 'standard' | 'symbols';
}

export interface RepoSearchQuery extends BaseProviderQuery {
  keywords?: string[];

  topics?: string[];

  owner?: string;

  minStars?: number;

  stars?: string;

  size?: string;

  created?: string;

  updated?: string;

  language?: string;

  match?: Array<'name' | 'description' | 'readme'>;

  archived?: boolean;

  visibility?: 'public' | 'private';

  forks?: string;

  license?: string;

  goodFirstIssues?: string;

  sort?: 'stars' | 'forks' | 'updated' | 'created' | 'best-match';

  order?: 'asc' | 'desc';

  limit?: number;

  page?: number;
}

export interface PullRequestQuery extends BaseProviderQuery {
  projectId?: string;

  owner?: string;

  repo?: string;

  query?: string;

  number?: number;

  state?: 'open' | 'closed' | 'merged' | 'all';

  author?: string;

  assignee?: string;

  commenter?: string;

  involves?: string;

  mentions?: string;

  reviewRequested?: string;

  reviewedBy?: string;

  labels?: string[];

  noLabel?: boolean;

  noMilestone?: boolean;

  noProject?: boolean;

  noAssignee?: boolean;

  baseBranch?: string;

  headBranch?: string;

  created?: string;

  updated?: string;

  closed?: string;

  mergedAt?: string;

  comments?: number | string;

  reactions?: number | string;

  interactions?: number | string;

  draft?: boolean;

  match?: Array<'title' | 'body' | 'comments'>;

  archived?: boolean;

  content?: unknown;

  reviewMode?: 'summary' | 'full';

  filePage?: number;

  commentPage?: number;

  commitPage?: number;

  itemsPerPage?: number;

  sort?: 'created' | 'updated' | 'best-match' | 'comments' | 'reactions';

  order?: 'asc' | 'desc';

  limit?: number;

  page?: number;

  charOffset?: number;

  charLength?: number;
}

export interface RepoStructureQuery extends BaseProviderQuery {
  projectId: string;

  ref?: string;

  path?: string;

  depth?: number;

  recursive?: boolean;

  itemsPerPage?: number;

  page?: number;

  includeSizes?: boolean;
}
