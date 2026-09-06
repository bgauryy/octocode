import type { NormalizedPrContentRequest } from './contentRequest.js';
import {
  shapeComments,
  shapeCommits,
  shapeReviews,
} from './contentResponse/commentsShaping.js';
import { shapeFileSurfaces } from './contentResponse/fileSurfaces.js';
import { nextCalls } from './contentResponse/nextCalls.js';
import { historyBodyView } from './contentResponse/contentView.js';
import {
  compactBody,
  continuationQuery,
  isRecord,
  pageContinuationQuery,
  paginateText,
  readPagination,
  readTextPagination,
  textContinuationQuery,
  type ContentPagination,
  type QueryLike,
  type TextPagination,
} from './contentResponse/pagination.js';

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
  shaped: Record<string, unknown>,
  surface: 'comments' | 'reviews' = 'comments'
): TextPagination | undefined {
  const comments = shaped[surface];
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
  rawPagination: ContentPagination,
  query: QueryLike,
  request: NormalizedPrContentRequest,
  prNumber: number
): ContentPagination | undefined {
  const contentPagination: ContentPagination = {};
  const bodyPagination = readTextPagination(rawPagination.body);
  const filePagination = readPagination(rawPagination.changedFiles);
  const commentPagination = readPagination(rawPagination.comments);
  const reviewPagination = readPagination(rawPagination.reviews);
  const commitPagination = readPagination(rawPagination.commits);
  const patchPagination = firstPatchPagination(shaped);
  const commentBodyPagination = firstCommentBodyPagination(shaped);
  const reviewBodyPagination = firstCommentBodyPagination(shaped, 'reviews');
  const fileContent = buildFileContentRequest(request);
  if (reviewBodyPagination && request.reviews) {
    contentPagination.reviewBody = {
      ...reviewBodyPagination,
      nextQuery: textContinuationQuery(
        query,
        prNumber,
        { reviews: true },
        reviewBodyPagination
      ),
    };
  }

  if (reviewPagination && request.reviews) {
    contentPagination.reviews = {
      ...reviewPagination,
      nextQuery: pageContinuationQuery(
        query,
        prNumber,
        { reviews: true },
        'reviewPage',
        reviewPagination
      ),
    };
  }

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
    const nextQuery = pageContinuationQuery(
      query,
      prNumber,
      fileContent,
      'filePage',
      filePagination
    );
    contentPagination.changedFiles = {
      ...filePagination,
      ...(nextQuery ? { nextQuery } : {}),
    };
  }

  if (commentPagination && request.comments) {
    const nextQuery = pageContinuationQuery(
      query,
      prNumber,
      { comments: request.comments },
      'commentPage',
      commentPagination
    );
    contentPagination.comments = {
      ...commentPagination,
      ...(nextQuery ? { nextQuery } : {}),
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
    const nextQuery = pageContinuationQuery(
      query,
      prNumber,
      { commits: request.commits },
      'commitPage',
      commitPagination
    );
    contentPagination.commits = {
      ...commitPagination,
      ...(nextQuery ? { nextQuery } : {}),
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

  const filePathsPagination = rawPagination.filePaths;
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
    rawBody && shouldMinify ? historyBodyView(rawBody, query) : rawBody;
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

  const { contentPagination: fileContentPagination = {}, ...fileSurfaces } =
    shapeFileSurfaces(pr, query, request);
  const {
    contentPagination: commentContentPagination = {},
    ...commentSurfaces
  } = shapeComments(pr, query, request);
  const { contentPagination: reviewContentPagination = {}, ...reviewSurfaces } =
    shapeReviews(pr, query, request);
  const { contentPagination: commitContentPagination = {}, ...commitSurfaces } =
    shapeCommits(pr, query, request);
  // Offer a selected-patch follow-up only when this response knows a path.
  const firstChangedFilePath = Array.isArray(fileSurfaces.changedFiles)
    ? (fileSurfaces.changedFiles as Array<{ path?: string }>).find(f => f.path)
        ?.path
    : undefined;

  const metadata = {
    number: pr.number,
    title: pr.title,
    state: pr.state,
    ...(pr.draft ? { draft: pr.draft } : {}),
    author: pr.author,
    ...(Array.isArray(pr.assignees) && pr.assignees.length
      ? { assignees: pr.assignees }
      : {}),
    ...(Array.isArray(pr.labels) && (pr.labels as unknown[]).length
      ? { labels: pr.labels }
      : {}),
    ...(pr.targetBranch ? { targetBranch: pr.targetBranch } : {}),
    ...(fullShape
      ? {
          ...(pr.sourceBranch ? { sourceBranch: pr.sourceBranch } : {}),
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
    ...(emitContentMap
      ? {
          next: nextCalls(query, prNumber, request, firstChangedFilePath),
        }
      : {}),
  };

  const shaped: Record<string, unknown> = {
    ...metadata,
    ...(Array.isArray(pr.providerLimits) && pr.providerLimits.length
      ? { providerLimits: pr.providerLimits }
      : {}),
    ...(request.body
      ? body
        ? { body: body.content }
        : { bodyEmpty: true }
      : {}),
    ...fileSurfaces,
    ...commentSurfaces,
    ...reviewSurfaces,
    ...commitSurfaces,
    ...(pr.reviewSummary ? { reviewSummary: pr.reviewSummary } : {}),
    ...(Array.isArray(pr.sanitizationWarnings) &&
    (pr.sanitizationWarnings as unknown[]).length > 0
      ? { sanitizationWarnings: pr.sanitizationWarnings }
      : {}),
  };
  const contentPagination = buildContentPagination(
    shaped,
    {
      ...(body ? { body: body.pagination } : {}),
      ...fileContentPagination,
      ...commentContentPagination,
      ...commitContentPagination,
      ...reviewContentPagination,
    },
    query,
    request,
    prNumber
  );
  if (contentPagination) shaped.contentPagination = contentPagination;
  return shaped;
}
