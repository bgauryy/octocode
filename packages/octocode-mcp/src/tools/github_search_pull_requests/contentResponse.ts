import type { NormalizedPrContentRequest } from './contentRequest.js';
import { applyContentViewMinification } from '../../utils/minifier/applyMinification.js';

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
};

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
  itemsPerPage = 20
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

function availableContent() {
  return {
    metadata: true,
    body: true,
    changedFiles: true,
    patches: true,
    comments: { discussion: true, reviewInline: true },
    reviews: true,
    commits: true,
  };
}

function nextCalls(query: QueryLike, prNumber: number) {
  const base = baseQuery(query, prNumber);
  return {
    getBody: { ...base, content: { body: true } },
    getChangedFiles: { ...base, content: { changedFiles: true } },
    getSelectedPatches: {
      ...base,
      content: {
        patches: { mode: 'selected', files: ['path/from/changedFiles'] },
      },
    },
    getAllPatches: { ...base, content: { patches: { mode: 'all' } } },
    getComments: {
      ...base,
      content: { comments: { discussion: true, reviewInline: true } },
    },
    getCommits: { ...base, content: { commits: { list: true } } },
    fullReview: { ...base, reviewMode: 'full' },
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
  const { items, pagination } = paginateItems(
    filtered,
    query.commentPage ?? query.page ?? 1,
    query.itemsPerPage ?? 20
  );
  return {
    comments: items.map(comment => {
      const body = paginateText(
        typeof comment.body === 'string' ? comment.body : '',
        query.charOffset ?? 0,
        query.charLength ?? 8_000
      );
      return {
        id: comment.id,
        author: comment.author,
        commentType: comment.commentType ?? 'discussion',
        path: comment.path,
        line: comment.line,
        bodyPreview: compactBody(
          typeof comment.body === 'string' ? comment.body : ''
        ),
        ...(body
          ? { body: body.content, bodyPagination: body.pagination }
          : {}),
        createdAt: comment.createdAt,
        updatedAt: comment.updatedAt,
      };
    }),
    commentPagination: pagination,
  };
}

function shapeReviews(
  pr: Record<string, unknown>,
  request: NormalizedPrContentRequest
) {
  if (!request.reviews) return {};
  const reviews = Array.isArray(pr.reviews)
    ? (pr.reviews as Array<Record<string, unknown>>)
    : [];
  return {
    reviews: reviews.map(review => ({
      id: review.id,
      user: review.user,
      state: review.state,
      bodyPreview: compactBody(
        typeof review.body === 'string' ? review.body : ''
      ),
      submittedAt: review.submittedAt ?? review.submitted_at,
      commitId: review.commitId ?? review.commit_id,
    })),
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
    query.itemsPerPage ?? 20
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
  const { items, pagination } = paginateItems(
    selected,
    query.filePage ?? query.page ?? 1,
    query.itemsPerPage ?? 20
  );

  const includePatch = request.patches.mode !== 'none';
  const shaped = items.map(change => {
    const base = shapeFileChange(change, false);
    if (!includePatch || typeof change.patch !== 'string') return base;
    const rawPatch = change.patch;
    const processedPatch =
      shouldMinify && typeof rawPatch === 'string'
        ? applyContentViewMinification(rawPatch, filePathOf(change))
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
  shouldMinify = true
): Record<string, unknown> {
  const prNumber = Number(pr.number);
  const body = request.body
    ? paginateText(
        typeof pr.body === 'string' ? pr.body : undefined,
        query.charOffset ?? 0,
        query.charLength ?? 12_000
      )
    : undefined;
  const metadata = {
    number: pr.number,
    title: pr.title,
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
    changedFilesCount: pr.changedFilesCount,
    additions: pr.additions,
    deletions: pr.deletions,
    bodyPreview: compactBody(typeof pr.body === 'string' ? pr.body : undefined),
    availableContent: availableContent(),
    next: nextCalls(query, prNumber),
  };

  return {
    ...metadata,
    ...(body ? { body: body.content, bodyPagination: body.pagination } : {}),
    ...shapeFileSurfaces(pr, query, request, shouldMinify),
    ...shapeComments(pr, query, request),
    ...shapeReviews(pr, request),
    ...shapeCommits(pr, query, request),
    ...(pr.reviewSummary ? { reviewSummary: pr.reviewSummary } : {}),
  };
}

export function buildContentHints(
  pullRequests: Array<Record<string, unknown>>,
  request: NormalizedPrContentRequest
): string[] {
  const first = pullRequests[0];
  if (!first) return [];
  const n = first.number;
  const hints = [
    `Content map included. For full PR review use prNumber=${n} reviewMode="full"; for specific surfaces use content.body, content.changedFiles, content.patches, content.comments, or content.commits.`,
  ];
  if (request.patches.mode === 'none') {
    hints.push(
      `Diffs are not included by default. Request selected files with content.patches={mode:"selected",files:["path"]}, or all patches paginated with content.patches={mode:"all"}.`
    );
  }
  if (!request.comments) {
    hints.push(
      `Comments are not included by default. Request content.comments={discussion:true,reviewInline:true}; add file="path" for file-specific inline comments.`
    );
  }
  return hints;
}
