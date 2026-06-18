import { PR_CONTENT_DEFAULT_ITEMS_PER_PAGE } from '../../config.js';
import type { NormalizedPrContentRequest } from './contentRequest.js';

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
  commentBodyOffset?: number;
  charLength?: number;
  matchString?: string;
};

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

type TextPagination = {
  charOffset: number;
  charLength: number;
  totalChars: number;
  hasMore: boolean;
  nextCharOffset?: number;
};

type ContentPaginationEntry = Record<string, unknown> & {
  hasMore: boolean;
  nextQuery?: Record<string, unknown>;
};

type ContentPagination = Partial<
  Record<
    | 'body'
    | 'changedFiles'
    | 'comments'
    | 'commentBody'
    | 'commits'
    | 'patches'
    | 'filePaths',
    ContentPaginationEntry
  >
>;

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

function baseQuery(query: QueryLike, prNumber: number) {
  return {
    owner: query.owner,
    repo: query.repo,
    prNumber,
  };
}

function compactQuery(value: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined)
  );
}

function continuationQuery(
  query: QueryLike,
  prNumber: number,
  patch: Record<string, unknown>
): Record<string, unknown> {
  return compactQuery({ ...baseQuery(query, prNumber), ...patch });
}

function textContinuationQuery(
  query: QueryLike,
  prNumber: number,
  content: Record<string, unknown>,
  pagination: TextPagination,
  extra: Record<string, unknown> = {}
): Record<string, unknown> | undefined {
  if (!pagination.hasMore || pagination.nextCharOffset === undefined) {
    return undefined;
  }

  return continuationQuery(query, prNumber, {
    content,
    ...extra,
    charOffset: pagination.nextCharOffset,
    charLength: query.charLength,
  });
}

function pageContinuationQuery(
  query: QueryLike,
  prNumber: number,
  content: Record<string, unknown>,
  pageKey: 'filePage' | 'commentPage' | 'commitPage',
  pagination: Pagination
): Record<string, unknown> | undefined {
  if (!pagination.hasMore || pagination.nextPage === undefined) {
    return undefined;
  }

  return continuationQuery(query, prNumber, {
    content,
    [pageKey]: pagination.nextPage,
    itemsPerPage: query.itemsPerPage,
  });
}

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
      const body = paginateText(
        typeof comment.body === 'string' ? comment.body : '',
        query.commentBodyOffset ?? 0,
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

function stripPatchComments(patch: string): string {
  return patch
    .split('\n')
    .filter(line => {
      if (!line.startsWith('+')) return true;
      const code = line.slice(1).trim();
      return (
        code !== '' &&
        !code.startsWith('//') &&
        !code.startsWith('/*') &&
        !code.startsWith('*')
      );
    })
    .map(line => {
      if (!line.startsWith('+')) return line;
      const code = line.slice(1);
      const stripped = code.replace(/\s*\/\/.*$/, '');
      return '+' + stripped.trimEnd();
    })
    .join('\n');
}

function normalizeMarkdownBody(body: string): string {
  return body.replace(/\n{3,}/g, '\n\n');
}

function shapeFileSurfaces(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  shouldMinify?: boolean
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
  const shaped = items.map(change => {
    const base = shapeFileChange(change, false);
    if (!includePatch || typeof change.patch !== 'string') return base;
    const rawPatch =
      shouldMinify && !needle ? stripPatchComments(change.patch) : change.patch;
    const patch = paginateText(
      rawPatch,
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readPagination(value: unknown): Pagination | undefined {
  if (!isRecord(value)) return undefined;
  if (
    typeof value.currentPage !== 'number' ||
    typeof value.totalPages !== 'number' ||
    typeof value.itemsPerPage !== 'number' ||
    typeof value.totalItems !== 'number' ||
    typeof value.hasMore !== 'boolean'
  ) {
    return undefined;
  }

  return value as Pagination;
}

function readTextPagination(value: unknown): TextPagination | undefined {
  if (!isRecord(value)) return undefined;
  if (
    typeof value.charOffset !== 'number' ||
    typeof value.charLength !== 'number' ||
    typeof value.totalChars !== 'number' ||
    typeof value.hasMore !== 'boolean'
  ) {
    return undefined;
  }

  return value as TextPagination;
}

function buildFileContentRequest(
  request: NormalizedPrContentRequest
): Record<string, unknown> {
  return {
    ...(request.changedFiles ? { changedFiles: true } : {}),
    ...(request.patches.mode !== 'none' ? { patches: request.patches } : {}),
  };
}

function firstPatchPagination(
  shaped: Record<string, unknown>
): TextPagination | undefined {
  const files = shaped.changedFiles;
  if (!Array.isArray(files)) return undefined;
  for (const file of files) {
    if (!isRecord(file)) continue;
    const pagination = readTextPagination(file.patchPagination);
    if (pagination?.hasMore) return pagination;
  }
  return undefined;
}

function firstCommentBodyPagination(
  shaped: Record<string, unknown>
): TextPagination | undefined {
  const comments = shaped.comments;
  if (!Array.isArray(comments)) return undefined;
  for (const comment of comments) {
    if (!isRecord(comment)) continue;
    const pagination = readTextPagination(comment.bodyPagination);
    if (pagination?.hasMore) return pagination;
  }
  return undefined;
}

function buildContentPagination(
  shaped: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  prNumber: number
): ContentPagination | undefined {
  const contentPagination: ContentPagination = {};
  const bodyPagination = readTextPagination(shaped.bodyPagination);
  const filePagination = readPagination(shaped.filePagination);
  const commentPagination = readPagination(shaped.commentPagination);
  const commitPagination = readPagination(shaped.commitPagination);
  const patchPagination = firstPatchPagination(shaped);
  const commentBodyPagination = firstCommentBodyPagination(shaped);
  const fileContent = buildFileContentRequest(request);

  if (bodyPagination) {
    contentPagination.body = {
      ...bodyPagination,
      nextQuery: textContinuationQuery(
        query,
        prNumber,
        { body: true },
        bodyPagination
      ),
    };
  }

  if (filePagination) {
    contentPagination.changedFiles = {
      ...filePagination,
      nextQuery: pageContinuationQuery(
        query,
        prNumber,
        fileContent,
        'filePage',
        filePagination
      ),
    };
  }

  if (commentPagination && request.comments) {
    contentPagination.comments = {
      ...commentPagination,
      nextQuery: pageContinuationQuery(
        query,
        prNumber,
        { comments: request.comments },
        'commentPage',
        commentPagination
      ),
    };
  }

  if (
    commentBodyPagination?.hasMore &&
    commentBodyPagination.nextCharOffset !== undefined &&
    request.comments
  ) {
    contentPagination.commentBody = {
      ...commentBodyPagination,
      nextQuery: continuationQuery(query, prNumber, {
        content: { comments: request.comments },
        ...(query.commentPage !== undefined
          ? { commentPage: query.commentPage }
          : {}),
        commentBodyOffset: commentBodyPagination.nextCharOffset,
        charLength: query.charLength,
      }),
    };
  }

  if (commitPagination && request.commits) {
    contentPagination.commits = {
      ...commitPagination,
      nextQuery: pageContinuationQuery(
        query,
        prNumber,
        { commits: request.commits },
        'commitPage',
        commitPagination
      ),
    };
  }

  if (patchPagination && request.patches.mode !== 'none') {
    contentPagination.patches = {
      ...patchPagination,
      nextQuery: textContinuationQuery(
        query,
        prNumber,
        { patches: request.patches },
        patchPagination,
        { filePage: query.filePage ?? query.page }
      ),
    };
  }

  const filePathsPagination = shaped.filePathsPagination;
  if (isRecord(filePathsPagination)) {
    contentPagination.filePaths = {
      ...filePathsPagination,
      hasMore: filePathsPagination.hasMore === true,
      nextQuery:
        filePathsPagination.hasMore === true
          ? continuationQuery(query, prNumber, {
              content: { changedFiles: true },
              filePage: filePathsPagination.nextFilePage,
            })
          : undefined,
    };
  }

  return Object.keys(contentPagination).length > 0
    ? contentPagination
    : undefined;
}

function removeLegacyPaginationFields(shaped: Record<string, unknown>): void {
  delete shaped.bodyPagination;
  delete shaped.filePagination;
  delete shaped.commentPagination;
  delete shaped.commitPagination;
  delete shaped.filePathsPagination;
}

export function shapePullRequestForContent(
  pr: Record<string, unknown>,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  shouldMinify?: boolean,
  showContentMap?: boolean
): Record<string, unknown> {
  const prNumber = Number(pr.number);
  const rawBody = typeof pr.body === 'string' ? pr.body : undefined;
  const processedBody =
    rawBody && shouldMinify ? normalizeMarkdownBody(rawBody) : rawBody;
  const body = request.body
    ? paginateText(
        processedBody,
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

  const isDetailFetch = (query as { prNumber?: number }).prNumber !== undefined;
  const fullShape =
    isDetailFetch || (query as { verbose?: boolean }).verbose === true;

  const metadata = {
    number: pr.number,
    title: pr.title,
    ...(pr.url ? { url: pr.url } : {}),
    state: pr.state,
    ...(pr.draft ? { draft: pr.draft } : {}),
    author: pr.author,
    ...(Array.isArray(pr.assignees) && pr.assignees.length
      ? { assignees: pr.assignees }
      : {}),
    ...(Array.isArray(pr.labels) && (pr.labels as unknown[]).length
      ? { labels: pr.labels }
      : {}),
    targetBranch: pr.targetBranch,
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
    ...(pr.changedFilesCount
      ? { changedFilesCount: pr.changedFilesCount }
      : {}),
    ...(pr.additions ? { additions: pr.additions } : {}),
    ...(pr.deletions ? { deletions: pr.deletions } : {}),
    ...(fullShape && !body
      ? {
          bodyPreview: compactBody(
            typeof pr.body === 'string' ? pr.body : undefined
          ),
        }
      : {}),
    ...(emitContentMap ? { next: nextCalls(query, prNumber, request) } : {}),
  };

  const shaped: Record<string, unknown> = {
    ...metadata,
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
    ...(Array.isArray(pr.sanitizationWarnings) &&
    (pr.sanitizationWarnings as unknown[]).length > 0
      ? { sanitizationWarnings: pr.sanitizationWarnings }
      : {}),
  };
  const contentPagination = buildContentPagination(
    shaped,
    query,
    request,
    prNumber
  );
  removeLegacyPaginationFields(shaped);
  if (contentPagination) shaped.contentPagination = contentPagination;
  return shaped;
}

export function buildContentHints(
  pullRequests: Array<Record<string, unknown>>,
  request: NormalizedPrContentRequest
): string[] {
  const first = pullRequests[0];
  if (!first) return [];
  const hints: string[] = [
    'PR response includes a `next` map — re-call with next.target + one content key (e.g. next.getBody, next.getAllPatches, next.getComments) to fetch a surface.',
  ];
  const contentPagination = isRecord(first.contentPagination)
    ? first.contentPagination
    : {};
  const continuationHints: Record<string, { label: string; field: string }> = {
    body: { label: 'body charOffset', field: 'charOffset' },
    changedFiles: { label: 'changedFiles filePage', field: 'filePage' },
    comments: { label: 'comments commentPage', field: 'commentPage' },
    commentBody: {
      label: 'comment body commentBodyOffset',
      field: 'commentBodyOffset',
    },
    commits: { label: 'commits commitPage', field: 'commitPage' },
    patches: { label: 'patches charOffset', field: 'charOffset' },
    filePaths: { label: 'file paths filePage', field: 'filePage' },
  };
  for (const [surface, { label, field }] of Object.entries(continuationHints)) {
    const entry = contentPagination[surface];
    if (!isRecord(entry) || entry.hasMore !== true) continue;
    const nextQuery = isRecord(entry.nextQuery) ? entry.nextQuery : {};
    const nextValue = nextQuery[field];
    if (typeof nextValue === 'number') {
      hints.push(`More PR ${label}=${nextValue}.`);
    }
  }
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
