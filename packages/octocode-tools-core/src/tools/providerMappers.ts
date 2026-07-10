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
  GitHubReposSearchSingleQuerySchema,
  GitHubViewRepoStructureQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import type { GitHubPullRequestSearchQueryLocalSchema } from './github_search_pull_requests/scheme.js';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../types/execution.js';

import {
  DEFAULT_MATCH_SNIPPET_CHARS,
  GITHUB_SEARCH_DEFAULT_LIMIT,
} from '../config.js';
import { GITHUB_STRUCTURE_DEFAULTS } from './github_view_repo_structure/constants.js';
import { FileContentQueryLocalSchema } from './github_fetch_content/scheme.js';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
type LocalFileContentQuery = z.infer<typeof FileContentQueryLocalSchema> & {
  minify: import('../scheme/fields.js').MinifyMode;
};

/**
 * Char-boundary truncation mirroring the Rust engine's `truncate_unicode`:
 * keeps at most `maxChars` Unicode scalars and appends `...` when it cuts.
 * Never slices UTF-8 mid-codepoint or mid-token — the single data-layer bound
 * for GitHub code-search match fragments (the render layer must not re-trim).
 */
export function truncateSnippetChars(
  value: string,
  maxChars = DEFAULT_MATCH_SNIPPET_CHARS
): string {
  if (maxChars <= 0) return '';
  const chars = [...value];
  if (chars.length <= maxChars) return value;
  if (maxChars <= 3) return '.'.repeat(maxChars);
  return chars.slice(0, maxChars - 3).join('') + '...';
}
type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
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
    reportedTotalMatches?: number;
    reachableTotalMatches?: number;
    totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
    totalMatchesCapped?: boolean;
  },
  label: string
): string[] {
  if (pagination.totalPages <= 1) {
    return [];
  }

  const hints: string[] = [];
  const perPage = pagination.entriesPerPage || pagination.perPage || 10;
  const totalMatches = pagination.totalMatches;
  const reportedTotalMatches = pagination.reportedTotalMatches;
  const reachableTotalMatches = pagination.reachableTotalMatches;
  const startItem = (pagination.currentPage - 1) * perPage + 1;
  const endItem =
    typeof totalMatches === 'number'
      ? Math.min(pagination.currentPage * perPage, totalMatches)
      : pagination.currentPage * perPage;
  const totalLabel =
    typeof totalMatches !== 'number'
      ? 'total unknown'
      : pagination.totalMatchesKind === 'lowerBound'
        ? `at least ${totalMatches}`
        : typeof reportedTotalMatches === 'number' &&
            typeof reachableTotalMatches === 'number' &&
            reportedTotalMatches > reachableTotalMatches
          ? `${reachableTotalMatches} reachable; GitHub reports ${reportedTotalMatches}`
          : `${totalMatches}`;

  if (pagination.hasMore) {
    hints.push(
      typeof totalMatches === 'number'
        ? `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${startItem}-${endItem} of ${totalLabel} ${label}). Next: page=${pagination.currentPage + 1}; page through before exhaustive claims.`
        : `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${startItem}-${endItem} ${label}; total unknown). Next: page=${pagination.currentPage + 1}; page through before exhaustive claims.`
    );
  }

  return hints;
}

export function mapCodeSearchToolQuery(
  query: WithOptionalMeta<GitHubCodeSearchQuery>
) {
  return {
    keywords: query.keywords ?? [],
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    path: query.path,
    filename: query.filename,
    extension: query.extension,
    language: (query as Record<string, unknown>).language as string | undefined,
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

  matchIndices?: Array<{ start: number; end: number; lineOffset: number }>;

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
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  hasMore: boolean;
  nextPage?: number;
  uniqueFileCount?: number;
}

export interface CodeSearchFlatResult {
  results: CodeSearchGroupedResult[];
  pagination?: CodeSearchPagination;

  nonExistentScope?: boolean;

  incompleteResults?: boolean;
}

function countMetadata(
  pagination:
    | {
        reportedTotalMatches?: number;
        reachableTotalMatches?: number;
        totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
        totalMatchesCapped?: boolean;
        uniqueFileCount?: number;
      }
    | undefined
) {
  return {
    ...(typeof pagination?.reportedTotalMatches === 'number'
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(typeof pagination?.reachableTotalMatches === 'number'
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination?.totalMatchesKind
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(typeof pagination?.totalMatchesCapped === 'boolean'
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    ...(typeof pagination?.uniqueFileCount === 'number'
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
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
      if (!m.context) continue;
      const match: CodeSearchGroupedMatch = {
        path: item.path,
        value: truncateSnippetChars(m.context),
      };
      if (m.positions?.length > 0) {
        match.matchIndices = m.positions.map(([start, end]) => ({
          start,
          end,
          lineOffset:
            (m.context ?? '').substring(0, start).split('\n').length - 1,
        }));
      }
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
    ...(data.incompleteResults ? { incompleteResults: true } : {}),
  };

  if (data.pagination && data.pagination.totalPages > 1) {
    result.pagination = {
      currentPage: data.pagination.currentPage,
      totalPages: data.pagination.totalPages,
      perPage: data.pagination.entriesPerPage || 10,
      totalMatches: data.pagination.totalMatches || 0,
      ...countMetadata(data.pagination),
      hasMore: data.pagination.hasMore,
      ...(data.pagination.hasMore
        ? { nextPage: data.pagination.currentPage + 1 }
        : {}),
    };
  }

  return result;
}

export function mapRepoSearchToolQuery(
  query: WithOptionalMeta<GitHubReposSearchSingleQuery>
) {
  const extra = query as Record<string, unknown>;
  return {
    keywords: query.keywords,
    topics: query.topicsToSearch,
    owner: query.owner,
    stars: query.stars,
    size: query.size,
    created: query.created,
    updated: query.updated,
    language: query.language,
    archived: extra.archived as boolean | undefined,
    visibility: extra.visibility as 'public' | 'private' | undefined,
    forks: extra.forks as string | undefined,
    license: extra.license as string | undefined,
    goodFirstIssues: extra.goodFirstIssues as string | undefined,
    match: query.match,
    sort: query.sort as
      'stars' | 'forks' | 'updated' | 'created' | 'best-match' | undefined,
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

function quotePRKeyword(kw: string): string {
  if (kw.startsWith('"')) return kw; // already quoted
  if (/\s/.test(kw)) return `"${kw.replace(/"/g, '\\"')}"`;
  return kw;
}

export function mapPullRequestToolQuery(query: PartialPRQuery) {
  const keywordParts = (query.keywordsToSearch ?? [])
    .filter(k => k.trim())
    .map(quotePRKeyword);
  const rawQuery = (query as { query?: string }).query?.trim() ?? '';
  const combinedQuery =
    [...keywordParts, ...(rawQuery ? [rawQuery] : [])].join(' ') || undefined;

  return {
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    query: combinedQuery,
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
    match: query.match,
    milestone: query.milestone,
    language: query.language,
    checks: query.checks,
    review: query.review,
    locked: query.locked,
    visibility: query.visibility,
    teamMentions: query['team-mentions'],
    project: query.project,
    archived: (query as Record<string, unknown>).archived as
      boolean | undefined,
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
    const fileChanges = pr.fileChanges;
    const originalFileChangeCount = fileChanges?.length ?? 0;
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
      ...(fileChanges && includeFileChanges ? { fileChanges } : {}),
      ...(Array.isArray(pr.sanitizationWarnings) &&
      pr.sanitizationWarnings.length > 0
        ? { sanitizationWarnings: pr.sanitizationWarnings }
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
        ...countMetadata(data.pagination),
        hasMore: data.pagination.hasMore,
        ...(data.pagination.hasMore
          ? { nextPage: data.pagination.currentPage + 1 }
          : {}),
      }
    : undefined;

  return {
    pullRequests,
    resultData: {
      pull_requests: pullRequests,
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
    forceRefresh: Boolean((query as { forceRefresh?: boolean }).forceRefresh),
    charOffset: query.charOffset,
    charLength: query.charLength,
    minify: query.minify,
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
    ...(typeof data.size === 'number' &&
      data.size > 0 && {
        fileSize: data.size,
      }),
    ...(typeof data.totalLines === 'number' && {
      totalLines: data.totalLines,
    }),
    ...(typeof data.sourceChars === 'number' && {
      sourceChars: data.sourceChars,
    }),
    ...(typeof data.sourceBytes === 'number' && {
      sourceBytes: data.sourceBytes,
    }),
    ...(data.contentView && {
      contentView: data.contentView,
    }),
    ...(data.isSkeleton === true && {
      isSkeleton: true,
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
    depth: typeof query.maxDepth === 'number' ? query.maxDepth : undefined,
    itemsPerPage:
      (query as { itemsPerPage?: number }).itemsPerPage ??
      GITHUB_STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE,
    page: (() => {
      const page = (query as { page?: number }).page;
      return typeof page === 'number' ? page : undefined;
    })(),
    includeSizes: (query as { includeSizes?: boolean }).includeSizes,
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

  const structureArray = Object.entries(filteredStructure)
    .sort(([a], [b]) => (a === '.' ? -1 : b === '.' ? 1 : a.localeCompare(b)))
    .map(([dir, entry]) => ({
      dir,
      files: entry.files,
      folders: entry.folders,
    }));

  const fileSizeMap = (
    data as { fileSizeMap?: Record<string, Record<string, number>> }
  ).fileSizeMap;
  const fileSizes: Record<string, number> = {};
  if (fileSizeMap) {
    for (const [dirPath, dirFiles] of Object.entries(fileSizeMap)) {
      if (filteredStructure[dirPath]) {
        const allowedFiles = new Set(filteredStructure[dirPath]!.files);
        for (const [fileName, size] of Object.entries(dirFiles)) {
          if (allowedFiles.has(fileName)) {
            // Key by full relative path so identically named files in
            // different directories don't collide onto one bare-name entry.
            const relativePath =
              dirPath === '.' ? fileName : `${dirPath}/${fileName}`;
            fileSizes[relativePath] = size;
          }
        }
      }
    }
  }

  // Filtering happens after provider pagination, so the provider's summary
  // counts ignored files/folders that were stripped. Recompute from the
  // filtered structure so the summary describes what is actually emitted.
  const filteredSummary = Object.values(filteredStructure).reduce(
    (totals, entry) => {
      totals.totalFiles += entry.files.length;
      totals.totalFolders += entry.folders.length;
      return totals;
    },
    { totalFiles: 0, totalFolders: 0 }
  );

  const resultData: Record<string, unknown> = {
    structure: structureArray,
    ...(Object.keys(fileSizes).length > 0 && { fileSizes }),
    summary: {
      totalFiles: filteredSummary.totalFiles,
      totalFolders: filteredSummary.totalFolders,
    },
  };

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

  if (
    data.pagination &&
    (data.pagination.hasMore || data.pagination.totalPages > 1)
  ) {
    resultData.pagination = data.pagination;
  }

  return resultData;
}
