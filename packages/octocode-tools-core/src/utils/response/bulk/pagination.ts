import type {
  BulkToolResponse,
  BulkResponsePagination,
} from '../../../types/bulk.js';

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
  const safeLength = Math.max(1, requestedLength);
  const safeOffset = Math.min(Math.max(0, requestedOffset), totalChars);
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
      currentPage,
      totalPages,
      hasMore,
      charOffset: safeOffset,
      charLength: endOffset - safeOffset,
      totalChars,
      ...(hasMore ? { nextCharOffset: endOffset } : {}),
    },
  };
}

export function appendResponsePagination<T extends Record<string, unknown>>(
  structuredContent: T,
  pagination?: NonNullable<BulkToolResponse['responsePagination']>
): T {
  if (!pagination) return structuredContent;
  // The responsePagination object carries the cursor; restating it as a hint is
  // redundant token waste. The page banner remains in the text channel header.
  return {
    ...structuredContent,
    responsePagination: pagination,
  };
}
