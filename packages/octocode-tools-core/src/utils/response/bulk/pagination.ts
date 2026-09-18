import { createHash } from 'node:crypto';
import type {
  BulkToolResponse,
  BulkResponsePagination,
} from '../../../types/bulk.js';

function responseSnapshot(text: string): string {
  return `response-v1:${createHash('sha256').update(text).digest('hex')}`;
}

function splitsSurrogatePair(text: string, offset: number): boolean {
  const before = text.charCodeAt(offset - 1);
  const after = text.charCodeAt(offset);
  return (
    before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff
  );
}

function chooseLineAwareEndOffset(
  text: string,
  startOffset: number,
  pageLength: number
): number {
  const rawEndOffset = Math.min(startOffset + pageLength, text.length);
  if (rawEndOffset >= text.length) return text.length;

  const minimumUsefulPageLength = Math.max(1, Math.floor(pageLength / 2));
  const boundaryOffsets = [
    text.lastIndexOf('\n', rawEndOffset - 1) + 1,
    text.lastIndexOf('\\n', rawEndOffset - 1) + 2,
  ].filter(offset => offset > startOffset && offset <= rawEndOffset);

  const bestBoundaryOffset = Math.max(...boundaryOffsets, -1);
  if (bestBoundaryOffset - startOffset >= minimumUsefulPageLength) {
    return bestBoundaryOffset;
  }

  if (splitsSurrogatePair(text, rawEndOffset)) {
    // A one-unit page must still return a whole code point and make progress.
    return rawEndOffset - startOffset === 1
      ? rawEndOffset + 1
      : rawEndOffset - 1;
  }
  return rawEndOffset;
}

function calculateLineAwarePageNumber(
  text: string,
  offset: number,
  pageLength: number
): number {
  let page = 1;
  let cursor = 0;

  while (cursor < offset && cursor < text.length) {
    const nextCursor = chooseLineAwareEndOffset(text, cursor, pageLength);
    if (nextCursor <= cursor) break;
    cursor = nextCursor;
    page += 1;
  }

  return cursor === offset ? page : Math.floor(offset / pageLength) + 1;
}

function calculateLineAwareTotalPages(
  text: string,
  pageLength: number
): number {
  if (text.length === 0) return 1;

  let pages = 0;
  let cursor = 0;

  while (cursor < text.length) {
    const nextCursor = chooseLineAwareEndOffset(text, cursor, pageLength);
    if (nextCursor <= cursor)
      return Math.max(1, Math.ceil(text.length / pageLength));
    cursor = nextCursor;
    pages += 1;
  }

  return Math.max(1, pages);
}

export function paginateBulkText(
  text: string,
  pagination?: BulkResponsePagination
): {
  text: string;
  pagination?: NonNullable<BulkToolResponse['responsePagination']>;
} {
  const requestedLength = pagination?.responseCharLength;
  const requestedOffset = pagination?.responseCharOffset ?? 0;
  if (requestedLength === undefined) {
    return { text };
  }

  const totalChars = text.length;
  const snapshot = responseSnapshot(text);
  const safeLength = Math.max(1, requestedLength);
  const safeOffset = Math.min(Math.max(0, requestedOffset), totalChars);

  const snapshotChanged = pagination?.responseSnapshot !== snapshot;
  const invalidBoundary = splitsSurrogatePair(text, safeOffset);
  if (requestedOffset > 0 && (snapshotChanged || invalidBoundary)) {
    const expectedSnapshot = pagination?.responseSnapshot;
    const reason = !snapshotChanged
      ? 'The requested offset splits a Unicode code point. Restart from responseCharOffset=0 and follow the returned continuation.'
      : expectedSnapshot
        ? 'The full response changed since the previous page. Discard earlier pages and restart from responseCharOffset=0.'
        : 'Later response pages require responseSnapshot from the previous page. Restart from responseCharOffset=0.';
    return {
      text: `# Response pagination restart required. ${reason}\n`,
      pagination: {
        scope: 'content.text',
        currentPage: 1,
        totalPages: calculateLineAwareTotalPages(text, safeLength),
        hasMore: true,
        charOffset: safeOffset,
        charLength: 0,
        totalChars,
        snapshot,
        ...(expectedSnapshot ? { expectedSnapshot } : {}),
        changed: Boolean(expectedSnapshot) && snapshotChanged,
        restart: true,
        nextCharOffset: 0,
      },
    };
  }

  const endOffset = chooseLineAwareEndOffset(text, safeOffset, safeLength);
  const hasMore = endOffset < totalChars;
  const currentPage = calculateLineAwarePageNumber(
    text,
    safeOffset,
    safeLength
  );
  const totalPages = calculateLineAwareTotalPages(text, safeLength);

  const pageText = text.slice(safeOffset, endOffset);
  const header = hasMore
    ? `# Response page ${currentPage}/${totalPages}. Next: responseCharOffset=${endOffset}\n`
    : `# Response page ${currentPage}/${totalPages}.\n`;

  return {
    text: `${header}${pageText}`,
    pagination: {
      scope: 'content.text',
      currentPage,
      totalPages,
      hasMore,
      charOffset: safeOffset,
      charLength: endOffset - safeOffset,
      totalChars,
      snapshot,
      ...(hasMore ? { nextCharOffset: endOffset } : {}),
    },
  };
}

export function appendResponsePagination<T extends Record<string, unknown>>(
  structuredContent: T,
  pagination?: NonNullable<BulkToolResponse['responsePagination']>,
  next?: NonNullable<BulkToolResponse['responsePagination']>['next']
): T {
  if (!pagination) return structuredContent;
  return {
    ...structuredContent,
    responsePagination: {
      ...pagination,
      ...(pagination.hasMore && next ? { next } : {}),
    },
  };
}

export function buildResponsePaginationContinuation(
  toolName: string,
  queries: object[],
  request: BulkResponsePagination | undefined,
  pagination: NonNullable<BulkToolResponse['responsePagination']> | undefined
): NonNullable<BulkToolResponse['responsePagination']>['next'] | undefined {
  if (!pagination?.hasMore || pagination.nextCharOffset === undefined) {
    return undefined;
  }
  const cleanQueries = queries.map(query => {
    const { goal: _goal, ...clean } = query as Record<string, unknown>;
    return clean;
  });
  return {
    tool: toolName,
    query: {
      queries: cleanQueries,
      responseCharLength: request?.responseCharLength,
      responseCharOffset: pagination.nextCharOffset,
      ...(pagination.restart ? {} : { responseSnapshot: pagination.snapshot }),
    },
  };
}
