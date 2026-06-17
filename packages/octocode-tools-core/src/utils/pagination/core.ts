import type { PaginationInfo } from '../../types/toolResults.js';
import type { PaginationMetadata, ApplyPaginationOptions } from './types.js';
import {
  byteToCharIndex,
  charToByteIndex,
  getByteLength,
  sliceContent,
} from '../file/byteOffset.js';

export function applyPagination(
  content: string,
  offset: number = 0,
  length?: number,
  options: ApplyPaginationOptions = {}
): PaginationMetadata {
  const mode = options.mode ?? 'characters';
  const totalChars = content.length;
  const totalBytes = getByteLength(content);

  if (length === undefined) {
    return {
      paginatedContent: content,
      byteOffset: 0,
      byteLength: totalBytes,
      totalBytes,
      nextByteOffset: undefined,
      charOffset: 0,
      charLength: totalChars,
      totalChars,
      nextCharOffset: undefined,
      hasMore: false,
      estimatedTokens: Math.ceil(content.length / 4),
      currentPage: 1,
      totalPages: 1,
    };
  }

  // Page counter divisor: use the caller's stable requested page size when
  // provided so the counter stays absolute even if each page's slice length
  // varies (semantic-boundary snapping). Falls back to the slice length.
  const pageSize = Math.max(1, options.pageSize ?? length);

  let paginatedContent: string;
  let startBytePos: number;
  let endBytePos: number;
  let startCharPos: number;
  let endCharPos: number;
  let hasMore: boolean;
  let currentPage: number;
  let totalPages: number;

  if (mode === 'bytes') {
    const requestedStartByte = Math.min(offset, totalBytes);
    const requestedEndByte = Math.min(requestedStartByte + length, totalBytes);

    startCharPos = byteToCharIndex(content, requestedStartByte);
    endCharPos = byteToCharIndex(content, requestedEndByte);

    if (
      endCharPos < totalChars &&
      charToByteIndex(content, endCharPos) < requestedEndByte
    ) {
      endCharPos += 1;
    }

    const slice = sliceContent(
      content,
      startCharPos,
      endCharPos - startCharPos
    );
    paginatedContent = slice.text;
    startCharPos = slice.charOffset;
    endCharPos = slice.charOffset + slice.charLength;
    startBytePos = slice.byteOffset;
    endBytePos = slice.byteOffset + slice.byteLength;

    hasMore = endBytePos < totalBytes;
    const pageOffset = options.actualOffset ?? requestedStartByte;
    currentPage = Math.floor(pageOffset / pageSize) + 1;
    totalPages = Math.max(currentPage, Math.ceil(totalBytes / pageSize));
  } else {
    const slice = sliceContent(content, offset, length);
    paginatedContent = slice.text;
    startCharPos = slice.charOffset;
    endCharPos = slice.charOffset + slice.charLength;
    startBytePos = slice.byteOffset;
    endBytePos = slice.byteOffset + slice.byteLength;

    hasMore = endCharPos < totalChars;
    const pageOffset = options.actualOffset ?? startCharPos;
    currentPage = Math.floor(pageOffset / pageSize) + 1;
    totalPages = Math.max(currentPage, Math.ceil(totalChars / pageSize));
  }

  return {
    paginatedContent,
    byteOffset: startBytePos,
    byteLength: endBytePos - startBytePos,
    totalBytes,
    nextByteOffset: hasMore ? endBytePos : undefined,
    charOffset: startCharPos,
    charLength: paginatedContent.length,
    totalChars,
    nextCharOffset: hasMore ? endCharPos : undefined,
    hasMore,
    estimatedTokens: Math.ceil(paginatedContent.length / 4),
    currentPage,
    totalPages,
  };
}

export function serializeForPagination(
  data: unknown,
  pretty: boolean = false
): string {
  return pretty ? JSON.stringify(data, null, 2) : JSON.stringify(data);
}

export function createPaginationInfo(
  metadata: PaginationMetadata
): PaginationInfo {
  return {
    currentPage: metadata.currentPage,
    totalPages: metadata.totalPages,
    hasMore: metadata.hasMore,
    charOffset: metadata.charOffset,
    charLength: metadata.charLength,
    totalChars: metadata.totalChars,
    ...(metadata.nextCharOffset !== undefined && {
      nextCharOffset: metadata.nextCharOffset,
    }),
  };
}

export interface SliceByCharResult {
  sliced: string;
  actualOffset: number;
  actualLength: number;
  hasMore: boolean;
  lineCount: number;
  totalChars: number;
  nextOffset?: number;
}

export function sliceByCharRespectLines(
  text: string,
  charOffset: number,
  charLength: number
): SliceByCharResult {
  const totalChars = text.length;

  if (totalChars === 0) {
    return {
      sliced: '',
      actualOffset: 0,
      actualLength: 0,
      hasMore: false,
      lineCount: 0,
      totalChars: 0,
    };
  }

  if (charOffset >= totalChars) {
    return {
      sliced: '',
      actualOffset: totalChars,
      actualLength: 0,
      hasMore: false,
      lineCount: 0,
      totalChars,
      nextOffset: totalChars,
    };
  }

  let actualOffset = charOffset;
  if (actualOffset > 0 && text[actualOffset - 1] !== '\n') {
    const prevNewline = text.lastIndexOf('\n', actualOffset - 1);
    actualOffset = prevNewline === -1 ? 0 : prevNewline + 1;
  }

  let endPos = actualOffset;
  let lineCount = 0;

  while (endPos < totalChars) {
    const nextNewline = text.indexOf('\n', endPos);
    if (nextNewline === -1) {
      endPos = totalChars;
      break;
    }
    endPos = nextNewline + 1; // include the \n
    lineCount++;
    if (endPos - actualOffset >= charLength) break;
  }

  const sliced = text.substring(actualOffset, endPos);
  const hasMore = endPos < totalChars;

  return {
    sliced,
    actualOffset,
    actualLength: sliced.length,
    hasMore,
    lineCount,
    totalChars,
    nextOffset: hasMore ? endPos : undefined,
  };
}
