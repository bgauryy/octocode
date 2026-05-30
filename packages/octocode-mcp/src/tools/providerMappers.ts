import type {
  CodeSearchResult,
  FileContentResult as ProviderFileContentResult,
  PullRequestSearchResult as ProviderPullRequestSearchResult,
  RepoSearchResult as ProviderRepoSearchResult,
  RepoStructureResult as ProviderRepoStructureResult,
} from '../providers/types.js';
import type { z } from 'zod/v4';
import type {
  FileContentQuerySchema,
  GitHubCodeSearchQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubViewRepoStructureQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../types/execution.js';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;

type PRDefaultKeys =
  | 'order'
  | 'limit'
  | 'page'
  | 'withComments'
  | 'withCommits'
  | 'type';
type PartialPRQuery = WithOptionalMeta<
  Omit<GitHubPullRequestSearchQuery, PRDefaultKeys> &
    Partial<Pick<GitHubPullRequestSearchQuery, PRDefaultKeys>>
>;
type PartialRepoStructureQuery = WithOptionalMeta<GitHubViewRepoStructureQuery>;

function toProviderProjectId(
  owner?: string,
  repo?: string
): string | undefined {
  return owner && repo ? `${owner}/${repo}` : undefined;
}

export function buildPaginationHints(
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    entriesPerPage?: number;
    perPage?: number;
    totalMatches?: number;
  },
  label: string
): string[] {
  if (pagination.totalPages <= 1) {
    return [];
  }

  const hints: string[] = [];
  const perPage = pagination.entriesPerPage || pagination.perPage || 10;
  const totalMatches = pagination.totalMatches || 0;
  const startItem = (pagination.currentPage - 1) * perPage + 1;
  const endItem = Math.min(pagination.currentPage * perPage, totalMatches);

  // Strict policy: emit only when there's more to fetch. Page/Previous/Jump
  // are data echoes of pagination.{currentPage,totalPages} the agent already
  // has — `Final page` is the same tautology in negative form.
  if (pagination.hasMore) {
    hints.push(
      `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${startItem}-${endItem} of ${totalMatches} ${label}). Next: page=${pagination.currentPage + 1}`
    );
  }

  return hints;
}

export function mapCodeSearchToolQuery(
  query: WithOptionalMeta<GitHubCodeSearchQuery>
) {
  return {
    keywords: query.keywordsToSearch ?? [],
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    path: query.path,
    filename: query.filename,
    extension: query.extension,
    match: query.match,
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export interface CodeSearchGroupedMatch {
  path: string;
  value?: string;
}

export interface CodeSearchGroupedResult {
  id: string;
  owner: string;
  repo: string;
  matches: CodeSearchGroupedMatch[];
}

export interface CodeSearchPagination {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalMatches: number;
  hasMore: boolean;
}

export interface CodeSearchFlatResult {
  results: CodeSearchGroupedResult[];
  pagination?: CodeSearchPagination;
}

function splitRepositoryPath(repositoryPath: string): {
  owner: string;
  repo: string;
} {
  const slashIdx = repositoryPath.lastIndexOf('/');
  if (slashIdx <= 0) {
    return { owner: '', repo: repositoryPath };
  }
  return {
    owner: repositoryPath.substring(0, slashIdx),
    repo: repositoryPath.substring(slashIdx + 1),
  };
}

export function mapCodeSearchProviderResult(
  data: CodeSearchResult,
  query: WithOptionalMeta<GitHubCodeSearchQuery>
): CodeSearchFlatResult {
  const isPathMatch = query.match === 'path';
  const groups = new Map<string, CodeSearchGroupedResult>();

  for (const item of data.items) {
    const repoFullName = item.repository.name || '';
    const { owner, repo } = splitRepositoryPath(repoFullName);
    const id = `${owner}/${repo}`;

    let group = groups.get(id);
    if (!group) {
      group = { id, owner, repo, matches: [] };
      groups.set(id, group);
    }

    if (isPathMatch || !item.matches?.length) {
      group.matches.push({ path: item.path });
      continue;
    }

    for (const m of item.matches) {
      group.matches.push({ path: item.path, value: m.context });
    }
  }

  const result: CodeSearchFlatResult = {
    results: Array.from(groups.values()),
  };

  if (data.pagination && data.pagination.totalPages > 1) {
    result.pagination = {
      currentPage: data.pagination.currentPage,
      totalPages: data.pagination.totalPages,
      perPage: data.pagination.entriesPerPage || 10,
      totalMatches: data.pagination.totalMatches || 0,
      hasMore: data.pagination.hasMore,
    };
  }

  return result;
}

export function mapRepoSearchToolQuery(
  query: WithOptionalMeta<GitHubReposSearchSingleQuery>
) {
  return {
    keywords: query.keywordsToSearch,
    topics: query.topicsToSearch,
    owner: query.owner,
    stars: query.stars,
    size: query.size,
    created: query.created,
    updated: query.updated,
    language: (query as Record<string, unknown>).language as string | undefined,
    match: query.match,
    sort: query.sort as
      | 'stars'
      | 'forks'
      | 'updated'
      | 'created'
      | 'best-match'
      | undefined,
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapRepoSearchProviderRepositories(
  repositories: ProviderRepoSearchResult['repositories']
): GitHubRepositoryOutput[] {
  const splitRepositoryPath = (repositoryPath: string) => {
    const slashIdx = repositoryPath.lastIndexOf('/');
    if (slashIdx <= 0) {
      return {
        owner: '',
        repo: repositoryPath,
      };
    }

    return {
      owner: repositoryPath.substring(0, slashIdx),
      repo: repositoryPath.substring(slashIdx + 1),
    };
  };

  return repositories.map(repo => {
    const { owner, repo: repoName } = splitRepositoryPath(repo.fullPath);
    return {
      owner: owner || '',
      repo: repoName || repo.name,
      defaultBranch: repo.defaultBranch,
      stars: repo.stars,
      description: repo.description || '',
      url: repo.url,
      createdAt: repo.createdAt,
      updatedAt: repo.updatedAt,
      pushedAt: repo.lastActivityAt,
      visibility: repo.visibility,
      topics: repo.topics,
      forksCount: repo.forks,
      openIssuesCount: repo.openIssuesCount,
      ...(repo.language && { language: repo.language }),
    };
  });
}

export function mapPullRequestToolQuery(query: PartialPRQuery) {
  return {
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    query: query.query,
    number: query.prNumber,
    state: query.state as 'open' | 'closed' | 'merged' | 'all' | undefined,
    author: query.author,
    assignee: query.assignee,
    commenter: query.commenter,
    involves: query.involves,
    mentions: query.mentions,
    reviewRequested: query['review-requested'],
    reviewedBy: query['reviewed-by'],
    labels: query.label
      ? Array.isArray(query.label)
        ? query.label
        : [query.label]
      : undefined,
    noLabel: query['no-label'],
    noMilestone: query['no-milestone'],
    noProject: query['no-project'],
    noAssignee: query['no-assignee'],
    baseBranch: query.base,
    headBranch: query.head,
    created: query.created,
    updated: query.updated,
    closed: query.closed,
    mergedAt: query['merged-at'],
    comments: query.comments,
    reactions: query.reactions,
    interactions: query.interactions,
    draft: query.draft,
    matchScope: query.matchScope as
      | Array<'title' | 'body' | 'comments'>
      | undefined,
    withComments: query.withComments,
    withCommits: query.withCommits,
    type: query.type as
      | 'metadata'
      | 'fullContent'
      | 'partialContent'
      | undefined,
    partialContentMetadata: query.partialContentMetadata,
    sort: query.sort as 'created' | 'updated' | 'best-match' | undefined,
    order: query.order as 'asc' | 'desc' | undefined,
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

const MAX_PR_BODY_LENGTH = 500;
const MAX_FILE_CHANGES_DEFAULT = 20;

function truncatePrBody(
  body: string | undefined | null,
  fullBody = false
): string | undefined {
  if (!body) return body ?? undefined;
  // A prNumber lookup is a targeted single-PR fetch — return the whole body
  // rather than the search preview. (This is also what the truncation hint
  // below promises, so it must actually hold for prNumber lookups.)
  if (fullBody || body.length <= MAX_PR_BODY_LENGTH) return body;
  return `${body.substring(0, MAX_PR_BODY_LENGTH)}... (${body.length} chars total, use prNumber for full body)`;
}

function capFileChanges(
  fileChanges: ProviderPullRequestSearchResult['items'][number]['fileChanges'],
  cap: number = MAX_FILE_CHANGES_DEFAULT
): {
  capped: typeof fileChanges;
  totalCount: number;
  wasTruncated: boolean;
} {
  if (!fileChanges)
    return { capped: undefined, totalCount: 0, wasTruncated: false };
  const totalCount = fileChanges.length;
  if (totalCount <= cap)
    return { capped: fileChanges, totalCount, wasTruncated: false };
  return { capped: fileChanges.slice(0, cap), totalCount, wasTruncated: true };
}

export function mapPullRequestProviderResultData(
  data: ProviderPullRequestSearchResult,
  options: { fullBody?: boolean } = {}
) {
  const { fullBody = false } = options;
  const pullRequests = data.items.map(pr => {
    const { capped: cappedFileChanges, totalCount: originalFileChangeCount } =
      capFileChanges(pr.fileChanges);
    return {
      number: pr.number,
      title: pr.title,
      body: truncatePrBody(pr.body, fullBody),
      url: pr.url,
      state: pr.state,
      draft: pr.draft,
      author: pr.author,
      assignees: pr.assignees,
      labels: pr.labels,
      sourceBranch: pr.sourceBranch,
      targetBranch: pr.targetBranch,
      sourceSha: pr.sourceSha,
      targetSha: pr.targetSha,
      createdAt: pr.createdAt,
      updatedAt: pr.updatedAt,
      closedAt: pr.closedAt,
      mergedAt: pr.mergedAt,
      commentsCount: pr.commentsCount,
      changedFilesCount: pr.changedFilesCount ?? originalFileChangeCount,
      additions: pr.additions,
      deletions: pr.deletions,
      ...(pr.comments && { comments: pr.comments }),
      ...(cappedFileChanges && { fileChanges: cappedFileChanges }),
    };
  });

  const pagination = data.pagination
    ? {
        currentPage: data.pagination.currentPage,
        totalPages: data.pagination.totalPages,
        perPage: data.pagination.entriesPerPage || 10,
        totalMatches: data.pagination.totalMatches || 0,
        hasMore: data.pagination.hasMore,
      }
    : undefined;

  return {
    pullRequests,
    resultData: {
      pull_requests: pullRequests,
      total_count: data.totalCount || pullRequests.length,
      ...(pagination && { pagination }),
    } as Record<string, unknown>,
    pagination,
  };
}

export function mapFileContentToolQuery(
  query: WithOptionalMeta<FileContentQuery>
) {
  const fullContent = Boolean(query.fullContent);

  return {
    projectId: `${query.owner}/${query.repo}`,
    path: String(query.path),
    ref: query.branch ? String(query.branch) : undefined,
    startLine: fullContent ? undefined : query.startLine,
    endLine: fullContent ? undefined : query.endLine,
    matchString:
      fullContent || !query.matchString ? undefined : String(query.matchString),
    matchStringContextLines: query.matchStringContextLines ?? 5,
    charOffset: query.charOffset ?? 0,
    charLength: query.charLength,
    fullContent,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapFileContentProviderResult(
  data: ProviderFileContentResult,
  query: WithOptionalMeta<FileContentQuery>
): Record<string, unknown> {
  return {
    path: data.path,
    content: data.content,
    ...(typeof data.totalLines === 'number' && {
      totalLines: data.totalLines,
    }),
    ...(data.isPartial && {
      isPartial: data.isPartial,
    }),
    ...(data.startLine && {
      startLine: data.startLine,
    }),
    ...(data.endLine && { endLine: data.endLine }),
    ...(data.lastModified && {
      lastModified: data.lastModified,
    }),
    ...(data.lastModifiedBy && {
      lastModifiedBy: data.lastModifiedBy,
    }),
    ...(data.pagination && {
      pagination: data.pagination,
    }),
    ...(data.warnings?.length && {
      warnings: data.warnings,
    }),
    ...(data.ref && query.branch !== data.ref
      ? { resolvedBranch: data.ref }
      : {}),
  };
}

export function mapRepoStructureToolQuery(
  query: PartialRepoStructureQuery,
  resolvedBranch: string
) {
  return {
    projectId: `${query.owner}/${query.repo}`,
    ref: resolvedBranch,
    path: query.path ? String(query.path) : undefined,
    depth: typeof query.depth === 'number' ? query.depth : undefined,
    entriesPerPage:
      typeof query.entriesPerPage === 'number'
        ? query.entriesPerPage
        : undefined,
    entryPageNumber:
      typeof query.entryPageNumber === 'number'
        ? query.entryPageNumber
        : undefined,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapRepoStructureProviderResult(
  data: ProviderRepoStructureResult,
  query: PartialRepoStructureQuery,
  filteredStructure: ProviderRepoStructureResult['structure'],
  resolvedBranch: string
): Record<string, unknown> {
  const requestedBranch = resolvedBranch;
  const actualBranch = data.branch ?? resolvedBranch;
  const branchFellBack =
    requestedBranch &&
    actualBranch &&
    requestedBranch !== actualBranch &&
    requestedBranch !== 'HEAD';

  const resultData: Record<string, unknown> = {
    structure: filteredStructure,
    summary: data.summary,
  };

  if (!query.branch && actualBranch) {
    resultData.resolvedBranch = actualBranch;
  }

  if (branchFellBack) {
    resultData.branchFallback = {
      requestedBranch,
      actualBranch,
      ...(data.defaultBranch !== undefined && {
        defaultBranch: data.defaultBranch,
      }),
      warning: `Branch '${requestedBranch}' not found. Showing '${actualBranch}' (default branch). Re-query with the correct branch name if branch-specific results are required.`,
    };
  }

  if (data.pagination) {
    resultData.pagination = data.pagination;
  }

  return resultData;
}
