import { PR_CONTENT_DEFAULT_ITEMS_PER_PAGE } from '../../config.js';
import type { NormalizedPrContentRequest } from './contentRequest.js';
import {
  applyContentViewMinification,
  minifyMarkdownCore,
} from '@octocodeai/octocode-minifier-utils';

type QueryLike = {
  owner?: string;
  repo?: string;
  prNumber?: number;
  page?: number;
  filePage?: number;
  commentPage?: number;
  commitPage?: number;
  itemsPerPage?: number;
  charOffset?: number;
  charLength?: number;
  matchString?: string;
};

// Case-insensitive keyword filter applied to cached PR content BEFORE
// pagination — lets agents search inside a large PR (file paths, patch text,
// comment/review bodies) the same way matchString narrows file reads.
function matchStringNeedle(query: QueryLike): string | undefined {
  const raw = query.matchString;
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim().toLowerCase();
  return trimmed.length > 0 ? trimmed : undefined;
}

function containsNeedle(value: unknown, needle: string): boolean {
  return typeof value === 'string' && value.toLowerCase().includes(needle);
}

type Pagination = {
  currentPage: number;
  totalPages: number;
  itemsPerPage: number;
  totalItems: number;
  hasMore: boolean;
  nextPage?: number;
};

function paginateItems<T>(
  items: T[],
  page = 1,
  itemsPerPage = PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
): {
  items: T[];
  pagination: Pagination;
} {
  const safePerPage = Math.min(Math.max(1, itemsPerPage), 100);
  const totalItems = items.length;
  const totalPages = Math.max(1, Math.ceil(totalItems / safePerPage));
  const currentPage = Math.min(Math.max(1, page), totalPages);
  const start = (currentPage - 1) * safePerPage;
  const end = Math.min(start + safePerPage, totalItems);
  return {
    items: items.slice(start, end),
    pagination: {
      currentPage,
      totalPages,
      itemsPerPage: safePerPage,
      totalItems,
      hasMore: currentPage < totalPages,
      ...(currentPage < totalPages ? { nextPage: currentPage + 1 } : {}),
    },
  };
}

function paginateText(
  value: string | undefined,
  charOffset = 0,
  charLength = 12_000
) {
  if (typeof value !== 'string') return undefined;
  const totalChars = value.length;
  const start = Math.min(Math.max(0, charOffset), totalChars);
  const length = Math.min(Math.max(1, charLength), 50_000);
  const end = Math.min(start + length, totalChars);
  const hasMore = end < totalChars;
  return {
    content: value.slice(start, end),
    pagination: {
      charOffset: start,
      charLength: end - start,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: end } : {}),
    },
  };
}

function compactBody(value: unknown, max = 500): string | undefined {
  if (typeof value !== 'string' || value.length === 0) return undefined;
  return value.length <= max ? value : `${value.slice(0, max - 3)}...`;
}

function stripDiffCommentOnlyLines(patch: string): string {
  return patch
    .split('\n')
    .filter(line => {
      if (line.startsWith('+++') || line.startsWith('---')) return true;
      return !/^[+\- ]\s*(?:\/\/|#(?!!)|--|;|%).*$/.test(line);
    })
    .join('\n');
}

function minifyPatchView(patch: string, filePath: string): string {
  return applyContentViewMinification(
    stripDiffCommentOnlyLines(patch),
    filePath
  );
}

function baseQuery(query: QueryLike, prNumber: number) {
  return {
    owner: query.owner,
    repo: query.repo,
    prNumber,
  };
}

// Entries already delivered in THIS response are omitted — the menu only
// offers escalations the agent does not have yet.
function nextCalls(
  query: QueryLike,
  prNumber: number,
  request: NormalizedPrContentRequest
) {
  return {
    target: baseQuery(query, prNumber),
    ...(request.body ? {} : { getBody: { content: { body: true } } }),
    ...(request.changedFiles
      ? {}
      : { getChangedFiles: { content: { changedFiles: true } } }),
    ...(request.patches.mode !== 'none'
      ? {}
      : {
          getSelectedPatches: {
            content: {
              patches: { mode: 'selected', files: ['path/from/changedFiles'] },
            },
          },
          getAllPatches: { content: { patches: { mode: 'all' } } },
        }),
    ...(request.comments
      ? {}
      : {
          getComments: {
            content: { comments: { discussion: true, reviewInline: true } },
          },
        }),
    ...(request.reviews ? {} : { getReviews: { content: { reviews: true } } }),
    ...(request.commits
      ? {}
      : { getCommits: { content: { commits: { list: true } } } }),
    ...(request.reviewMode === 'full'
      ? {}
      : { fullReview: { reviewMode: 'full' } }),
  };
}

function filePathOf(change: Record<string, unknown>): string {
  return String(change.path ?? change.filename ?? '');
}

function shapeFileChange(
  change: Record<string, unknown>,
  includePatch: boolean
) {
  return {
    path: filePathOf(change),
    status: String(change.status ?? ''),
    additions: Number(change.additions ?? 0),
    deletions: Number(change.deletions ?? 0),
    ...(includePatch && typeof change.patch === 'string'
      ? { patch: change.patch }
      : {}),
  };
}

type CommentRequest = Exclude<NormalizedPrContentRequest['comments'], false>;

function filterComments(
  comments: Array<Record<string, unknown>>,
  request: CommentRequest
): Array<Record<string, unknown>> {
  return comments.filter(comment => {
    const type = comment.commentType;
    if (request.file && comment.path !== request.file) return false;
    if (type === 'review_inline') return request.reviewInline;
    return request.discussion;
  });
}

function shapeComments(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.comments) return {};
  const allComments = Array.isArray(pr.comments)
    ? (pr.comments as Array<Record<string, unknown>>)
    : [];
  const filtered = filterComments(allComments, request.comments);
  const needle = matchStringNeedle(query);
  const matched = needle
    ? filtered.filter(comment => containsNeedle(comment.body, needle))
    : filtered;
  const { items, pagination } = paginateItems(
    matched,
    query.commentPage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
  );
  return {
    comments: items.map(comment => {
      // Each comment body always starts at offset 0 — charOffset from the
      // query is for the PR body continuation only, not per-comment pagination.
      const body = paginateText(
        typeof comment.body === 'string' ? comment.body : '',
        0,
        query.charLength ?? 12_000
      );
      return {
        id: comment.id,
        author: comment.author,
        commentType: comment.commentType ?? 'discussion',
        path: comment.path,
        line: comment.line,
        ...(comment.in_reply_to_id != null
          ? { in_reply_to_id: comment.in_reply_to_id }
          : {}),
        // bodyPreview is only a fallback — when the full (paginated) body is
        // included it would duplicate the same text verbatim.
        ...(body
          ? { body: body.content, bodyPagination: body.pagination }
          : {
              bodyPreview: compactBody(
                typeof comment.body === 'string' ? comment.body : ''
              ),
            }),
        createdAt: comment.createdAt,
        updatedAt: comment.updatedAt,
      };
    }),
    commentPagination: pagination,
  };
}

function shapeReviews(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.reviews) return {};
  const allReviews = Array.isArray(pr.reviews)
    ? (pr.reviews as Array<Record<string, unknown>>)
    : [];
  const needle = matchStringNeedle(query);
  const reviews = needle
    ? allReviews.filter(review => containsNeedle(review.body, needle))
    : allReviews;
  return {
    reviews: reviews.map(review => {
      // Paginate the review body. charOffset=0 always (review bodies start
      // fresh — charOffset is for PR body continuation only).
      const rawBody = typeof review.body === 'string' ? review.body : '';
      const paginated = paginateText(
        rawBody || undefined,
        0,
        query.charLength ?? 12_000
      );
      return {
        id: review.id,
        user: review.user,
        state: review.state,
        ...(paginated
          ? { body: paginated.content, bodyPagination: paginated.pagination }
          : {}),
        submittedAt: review.submittedAt ?? review.submitted_at,
        commitId: review.commitId ?? review.commit_id,
      };
    }),
  };
}

function shapeCommits(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest
) {
  if (!request.commits) return {};
  const allCommits = Array.isArray(pr.commits)
    ? (pr.commits as Array<Record<string, unknown>>)
    : [];
  const { items, pagination } = paginateItems(
    allCommits,
    query.commitPage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
  );
  return {
    commits: items.map(commit => ({
      sha: commit.sha,
      message: commit.message,
      author: commit.author,
      date: commit.date,
      ...(request.commits &&
      request.commits.includeFiles &&
      Array.isArray(commit.files)
        ? { files: commit.files }
        : {}),
    })),
    commitPagination: pagination,
  };
}

function shapeFileSurfaces(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  shouldMinify = true
) {
  const allChanges = Array.isArray(pr.fileChanges)
    ? (pr.fileChanges as Array<Record<string, unknown>>)
    : [];
  const files = request.patches.files;
  const selected =
    files && files.length > 0
      ? allChanges.filter(change => files.includes(filePathOf(change)))
      : allChanges;
  const needle = matchStringNeedle(query);
  const matched = needle
    ? selected.filter(
        change =>
          containsNeedle(filePathOf(change), needle) ||
          containsNeedle(change.patch, needle)
      )
    : selected;
  const { items, pagination } = paginateItems(
    matched,
    query.filePage ?? query.page ?? 1,
    query.itemsPerPage ?? PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
  );

  const includePatch = request.patches.mode !== 'none';
  // When matchString is active, skip minification so the displayed patch
  // contains the matched text. Minification could otherwise strip the very
  // lines that caused the file to match (e.g. comment-only diff lines).
  const effectiveMinify = shouldMinify && !needle;
  const shaped = items.map(change => {
    const base = shapeFileChange(change, false);
    if (!includePatch || typeof change.patch !== 'string') return base;
    const rawPatch = change.patch;
    const processedPatch =
      effectiveMinify && typeof rawPatch === 'string'
        ? minifyPatchView(rawPatch, filePathOf(change))
        : rawPatch;
    const patch = paginateText(
      processedPatch,
      query.charOffset ?? 0,
      query.charLength ?? 12_000
    );
    return {
      ...base,
      patch: patch?.content ?? '',
      ...(patch ? { patchPagination: patch.pagination } : {}),
    };
  });

  if (request.changedFiles || request.patches.mode !== 'none') {
    return {
      changedFiles: shaped,
      filePagination: pagination,
    };
  }

  if (allChanges.length === 0) return {};

  return {
    filePathsPreview: allChanges.slice(0, 20).map(filePathOf).filter(Boolean),
    filePathsPagination: {
      totalFiles: allChanges.length,
      filesPerPage: 20,
      hasMore: allChanges.length > 20,
      ...(allChanges.length > 20 ? { nextFilePage: 2 } : {}),
    },
  };
}

export function shapePullRequestForContent(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  // shouldMinify defaults false here — callers pass the schema-resolved minify value.
  shouldMinify = false,
  showContentMap?: boolean
): Record<string, unknown> {
  const prNumber = Number(pr.number);
  const body = request.body
    ? paginateText(
        (() => {
          const raw = typeof pr.body === 'string' ? pr.body : undefined;
          if (!raw) return undefined;
          // Apply markdown minification when the standard (token-saving) view
          // is requested. minify:"none" opts out for exact-text quoting.
          return shouldMinify ? minifyMarkdownCore(raw) : raw;
        })(),
        query.charOffset ?? 0,
        query.charLength ?? 12_000
      )
    : undefined;
  const hasContent =
    request.body ||
    request.changedFiles ||
    request.patches.mode !== 'none' ||
    Boolean(request.comments) ||
    request.reviews ||
    Boolean(request.commits);
  const emitContentMap =
    showContentMap !== undefined ? showContentMap : hasContent;

  // List searches are lean unless verbose=true: url is derivable from
  // owner/repo/number, bodyPreview belongs to the detail view, updatedAt and
  // closedAt-when-merged are near-duplicates of mergedAt. prNumber detail
  // fetches always emit the full shape.
  const isDetailFetch = (query as { prNumber?: number }).prNumber !== undefined;
  const fullShape =
    isDetailFetch || (query as { verbose?: boolean }).verbose === true;

  const metadata = {
    number: pr.number,
    title: pr.title,
    ...(fullShape ? { url: pr.url } : {}),
    state: pr.state,
    // draft only emitted when true (false is the normal case, wastes tokens)
    ...(pr.draft ? { draft: pr.draft } : {}),
    author: pr.author,
    ...(Array.isArray(pr.assignees) && pr.assignees.length
      ? { assignees: pr.assignees }
      : {}),
    // labels omitted when empty
    ...(Array.isArray(pr.labels) && (pr.labels as unknown[]).length
      ? { labels: pr.labels }
      : {}),
    targetBranch: pr.targetBranch,
    // verbose: expose branch details and SHA for precise checkout/diff context
    ...(fullShape
      ? {
          sourceBranch: pr.sourceBranch,
          ...(pr.sourceSha ? { sourceSha: pr.sourceSha } : {}),
        }
      : {}),
    createdAt: pr.createdAt,
    ...(fullShape ? { updatedAt: pr.updatedAt } : {}),
    ...(fullShape || !pr.mergedAt ? { closedAt: pr.closedAt } : {}),
    mergedAt: pr.mergedAt,
    ...(pr.commentsCount ? { commentsCount: pr.commentsCount } : {}),
    changedFilesCount: pr.changedFilesCount,
    // additions/deletions omitted when zero
    ...(pr.additions ? { additions: pr.additions } : {}),
    ...(pr.deletions ? { deletions: pr.deletions } : {}),
    // bodyPreview is only a fallback — omitted when the full (paginated)
    // body is part of this response.
    ...(fullShape && !body
      ? {
          bodyPreview: compactBody(
            typeof pr.body === 'string' ? pr.body : undefined
          ),
        }
      : {}),
    ...(emitContentMap ? { next: nextCalls(query, prNumber, request) } : {}),
  };

  return {
    ...metadata,
    // When body was explicitly requested but the PR has no description,
    // emit bodyEmpty:true so the agent knows it was fetched (not just missing).
    ...(request.body
      ? body
        ? { body: body.content, bodyPagination: body.pagination }
        : { bodyEmpty: true }
      : {}),
    ...shapeFileSurfaces(pr, query, request, shouldMinify),
    ...shapeComments(pr, query, request),
    ...shapeReviews(pr, query, request),
    ...shapeCommits(pr, query, request),
    ...(pr.reviewSummary ? { reviewSummary: pr.reviewSummary } : {}),
    // Warnings from bot filtering, secret redaction — must reach agent output.
    ...(Array.isArray(pr.sanitizationWarnings) &&
    (pr.sanitizationWarnings as unknown[]).length > 0
      ? { sanitizationWarnings: pr.sanitizationWarnings }
      : {}),
  };
}

export function buildContentHints(
  pullRequests: Array<Record<string, unknown>>,
  request: NormalizedPrContentRequest
): string[] {
  const first = pullRequests[0];
  if (!first) return [];
  const hints: string[] = [
    'Use next.target + a content key to fetch body, changedFiles, patches, comments, or commits.',
  ];
  if (request.patches.mode === 'none') {
    hints.push(
      'Patches not included — request content.patches={mode:"all"} or {mode:"selected",files:[...]}.'
    );
  }
  if (!request.comments) {
    hints.push(
      'Comments not included — request content.comments={discussion:true,reviewInline:true}.'
    );
  }
  return hints;
}
