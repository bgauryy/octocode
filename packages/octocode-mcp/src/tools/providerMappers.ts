import type {
  CodeSearchResult,
  FileContentResult as ProviderFileContentResult,
  PullRequestSearchResult as ProviderPullRequestSearchResult,
  RepoSearchResult as ProviderRepoSearchResult,
  RepoStructureResult as ProviderRepoStructureResult,
} from '../providers/types.js';
import type { z } from 'zod';
import type {
  GitHubCodeSearchQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubViewRepoStructureQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../types/execution.js';

import { GITHUB_SEARCH_DEFAULT_LIMIT } from '../config.js';
import { getOutputMinifyDefault } from '../utils/pagination/charLimit.js';
import { GITHUB_STRUCTURE_DEFAULTS } from './github_view_repo_structure/constants.js';
import { FileContentQueryLocalSchema } from './github_fetch_content/scheme.js';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
// `minify` is optional at the type level: the provider mapper applies the
// configured content-view default so local and GitHub file reads stay aligned.
type LocalFileContentQuery = Omit<
  z.infer<typeof FileContentQueryLocalSchema>,
  'minify'
> & { minify?: import('../scheme/localSchemaOverlay.js').MinifyMode };
type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;

type PRDefaultKeys = 'order' | 'limit' | 'page';
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
  const totalMatches = pagination.totalMatches;
  const startItem = (pagination.currentPage - 1) * perPage + 1;
  const endItem =
    typeof totalMatches === 'number'
      ? Math.min(pagination.currentPage * perPage, totalMatches)
      : pagination.currentPage * perPage;

  if (pagination.hasMore) {
    hints.push(
      typeof totalMatches === 'number'
        ? `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${startItem}-${endItem} of ${totalMatches} ${label}). Next: page=${pagination.currentPage + 1}`
        : `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${startItem}-${endItem} ${label}; total unknown). Next: page=${pagination.currentPage + 1}`
    );
    hints.push(
      `Results are paginated — use page=2, page=3 … to retrieve all ${label} before reporting a total count or enumerating exhaustively.`
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
    limit: (query as Record<string, unknown>).limit as number | undefined,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export interface CodeSearchGroupedMatch {
  path: string;
  value?: string;

  pathOnly?: boolean;

  matchIndices?: Array<{ start: number; end: number }>;

  /** verbose mode: html URL of the matched file. */
  url?: string;
}

export interface CodeSearchGroupedResult {
  id: string;
  queryId?: string;
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

  nonExistentScope?: boolean;
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
  const verbose = (query as { verbose?: boolean }).verbose === true;
  const groups = new Map<string, CodeSearchGroupedResult>();

  for (const item of data.items) {
    const repoFullName = item.repository.name || '';
    const { owner, repo } = splitRepositoryPath(repoFullName);
    const id = `${owner}/${repo}`;

    const itemExtra = item as { url?: string };
    let group = groups.get(id);
    if (!group) {
      group = { id, owner, repo, matches: [] };
      groups.set(id, group);
    }

    if (isPathMatch || !item.matches?.length) {
      group.matches.push({
        path: item.path,
        ...(!isPathMatch ? { pathOnly: true } : {}),
        ...(verbose && itemExtra.url ? { url: itemExtra.url } : {}),
      });
      continue;
    }

    let firstMatchForItem = true;
    let emittedMatchForItem = false;
    for (const m of item.matches) {
      // Empty snippet text: matchIndices would point into nothing — drop the
      // match entry entirely (the file falls back to a pathOnly entry below).
      if (!m.context) continue;
      const match: CodeSearchGroupedMatch = {
        path: item.path,
        value: m.context,
      };
      if (m.positions?.length > 0) {
        match.matchIndices = m.positions.map(([start, end]) => ({
          start,
          end,
        }));
      }
      // verbose: emit the file URL once per file, not per fragment
      if (verbose && firstMatchForItem && itemExtra.url) {
        match.url = itemExtra.url;
        firstMatchForItem = false;
      }
      group.matches.push(match);
      emittedMatchForItem = true;
    }

    if (!emittedMatchForItem) {
      group.matches.push({
        path: item.path,
        pathOnly: true,
        ...(verbose && itemExtra.url ? { url: itemExtra.url } : {}),
      });
    }
  }

  const result: CodeSearchFlatResult = {
    results: Array.from(groups.values()),
    ...(data.nonExistentScope ? { nonExistentScope: true } : {}),
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
    archived: (query as Record<string, unknown>).archived as
      | boolean
      | undefined,
    match: query.match,
    sort: query.sort as
      | 'stars'
      | 'forks'
      | 'updated'
      | 'created'
      | 'best-match'
      | undefined,
    limit: (query as Record<string, unknown>).limit as number | undefined,
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
    labels: (() => {
      const labelValue = query.label;
      if (!labelValue) return undefined;
      return Array.isArray(labelValue) ? labelValue : [labelValue];
    })(),
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
    archived: (query as Record<string, unknown>).archived as
      | boolean
      | undefined,
    content: (query as { content?: unknown }).content,
    reviewMode: (query as { reviewMode?: 'summary' | 'full' }).reviewMode,
    filePage: (query as { filePage?: number }).filePage,
    commentPage: (query as { commentPage?: number }).commentPage,
    commitPage: (query as { commitPage?: number }).commitPage,
    itemsPerPage: (query as { itemsPerPage?: number }).itemsPerPage,
    sort: query.sort as
      | 'created'
      | 'updated'
      | 'best-match'
      | 'comments'
      | 'reactions'
      | undefined,
    order: query.order as 'asc' | 'desc' | undefined,
    limit: (query as { limit?: number }).limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
    page: query.page,
    charOffset: (query as { charOffset?: number }).charOffset,
    charLength: (query as { charLength?: number }).charLength,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

function capFileChanges(
  fileChanges: ProviderPullRequestSearchResult['items'][number]['fileChanges']
): {
  capped: typeof fileChanges;
  totalCount: number;
  wasTruncated: boolean;
} {
  if (!fileChanges)
    return { capped: undefined, totalCount: 0, wasTruncated: false };
  return {
    capped: fileChanges,
    totalCount: fileChanges.length,
    wasTruncated: false,
  };
}

type ProviderPrComment = NonNullable<
  ProviderPullRequestSearchResult['items'][number]['comments']
>[number];

function detectReviewThemes(comments: readonly ProviderPrComment[]): string[] {
  const bodies = comments.map(comment => comment.body.toLowerCase());
  const themes: string[] = [];

  if (
    bodies.some(body => /\b(lgtm|looks good|approved|ship it)\b/.test(body))
  ) {
    themes.push('approval');
  }
  if (
    bodies.some(body =>
      /\b(change|fix|concern|blocker|blocking|request changes?)\b/.test(body)
    )
  ) {
    themes.push('changes-requested');
  }
  if (bodies.some(body => body.includes('?'))) {
    themes.push('question');
  }

  return themes.length > 0 ? themes : ['discussion'];
}

function buildReviewSummary(
  comments: readonly ProviderPrComment[] | undefined
):
  | {
      totalComments: number;
      inlineComments: number;
      discussionComments: number;
      commenters: string[];
      latestCommentAt?: string;
      themes: string[];
    }
  | undefined {
  if (!comments || comments.length === 0) return undefined;
  const commenters = Array.from(
    new Set(comments.map(comment => comment.author))
  );
  const latestCommentAt = comments
    .map(comment => comment.updatedAt || comment.createdAt)
    .filter(Boolean)
    .sort()
    .at(-1);
  const inlineComments = comments.filter(
    c =>
      (c as ProviderPrComment & { commentType?: string }).commentType ===
      'review_inline'
  ).length;
  return {
    totalComments: comments.length,
    inlineComments,
    discussionComments: comments.length - inlineComments,
    commenters: commenters.slice(0, 8),
    ...(latestCommentAt ? { latestCommentAt } : {}),
    themes: detectReviewThemes(comments),
  };
}

export function mapPullRequestProviderResultData(
  data: ProviderPullRequestSearchResult,
  options: { includeFileChanges?: boolean } = {}
) {
  const { includeFileChanges = true } = options;
  const pullRequests = data.items.map(pr => {
    const { capped: cappedFileChanges, totalCount: originalFileChangeCount } =
      capFileChanges(pr.fileChanges);
    const comments = Array.isArray(pr.comments) ? pr.comments : undefined;
    const reviewSummary = buildReviewSummary(comments);
    return {
      number: pr.number,
      title: pr.title,
      body: pr.body ?? undefined,
      ...(pr.bodyPagination && { bodyPagination: pr.bodyPagination }),
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
      ...(Array.isArray(pr.comments) &&
        pr.comments.length > 0 && {
          comments: pr.comments.map(comment => ({
            ...comment,
            ...(comment.bodyPagination && {
              bodyPagination: comment.bodyPagination,
            }),
          })),
        }),
      ...(pr.reviews && { reviews: pr.reviews }),
      ...(pr.commits && { commits: pr.commits }),
      ...(reviewSummary && { reviewSummary }),
      ...(cappedFileChanges && includeFileChanges
        ? { fileChanges: cappedFileChanges }
        : {}),
    };
  });

  const pagination = data.pagination
    ? {
        currentPage: data.pagination.currentPage,
        totalPages: data.pagination.totalPages,
        perPage: data.pagination.entriesPerPage || 10,
        ...(typeof data.pagination.totalMatches === 'number'
          ? { totalMatches: data.pagination.totalMatches }
          : {}),
        hasMore: data.pagination.hasMore,
      }
    : undefined;

  return {
    pullRequests,
    resultData: {
      pull_requests: pullRequests,
      // pagination.totalMatches already carries the count — only emit
      // total_count when there is no pagination block to read it from.
      ...(pagination
        ? { pagination }
        : { total_count: data.totalCount || pullRequests.length }),
    } as Record<string, unknown>,
    pagination,
  };
}

export function mapFileContentToolQuery(query: LocalFileContentQuery) {
  const fullContent = Boolean(query.fullContent);

  return {
    projectId: `${query.owner}/${query.repo}`,
    path: String(query.path),
    ref: query.branch ? String(query.branch) : undefined,
    startLine: fullContent ? undefined : query.startLine,
    endLine: fullContent ? undefined : query.endLine,
    matchString:
      fullContent || !query.matchString ? undefined : String(query.matchString),
    contextLines: (query as { contextLines?: number }).contextLines ?? 5,
    fullContent,
    charOffset: query.charOffset,
    charLength: query.charLength,
    minify: query.minify ?? getOutputMinifyDefault(),
    matchStringIsRegex: query.matchStringIsRegex,
    matchStringCaseSensitive: query.matchStringCaseSensitive,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapFileContentProviderResult(
  data: ProviderFileContentResult,
  query: WithOptionalMeta<LocalFileContentQuery>
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
    ...(data.matchRanges?.length && { matchRanges: data.matchRanges }),
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
    ...(data.matchNotFound === true && {
      matchNotFound: true,
    }),
    ...(data.searchedFor && {
      searchedFor: data.searchedFor,
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
    itemsPerPage:
      (query as { itemsPerPage?: number }).itemsPerPage ??
      GITHUB_STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
    page: (() => {
      const page = (query as { page?: number }).page;
      return typeof page === 'number' ? page : undefined;
    })(),
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export function mapRepoStructureProviderResult(
  data: ProviderRepoStructureResult,
  _query: PartialRepoStructureQuery,
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

  // Echo the served ref consistently — also when the caller passed an
  // explicit branch/tag/SHA, so every response states which ref it reflects.
  if (actualBranch) {
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
