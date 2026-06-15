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

    // byteToCharIndex rounds DOWN when a byte boundary falls mid-character.
    // Round UP instead so we always emit complete characters — never malformed UTF-8.
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
    currentPage = Math.floor(pageOffset / length) + 1;
    totalPages = Math.ceil(totalBytes / length);
  } else {
    const slice = sliceContent(content, offset, length);
    paginatedContent = slice.text;
    startCharPos = slice.charOffset;
    endCharPos = slice.charOffset + slice.charLength;
    startBytePos = slice.byteOffset;
    endBytePos = slice.byteOffset + slice.byteLength;

    hasMore = endCharPos < totalChars;
    const pageOffset = options.actualOffset ?? startCharPos;
    currentPage = Math.floor(pageOffset / length) + 1;
    totalPages = Math.ceil(totalChars / length);
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

/**
 * Slices `text` starting at `charOffset` (snapped back to the nearest line
 * start when mid-line), collecting complete lines until at least `charLength`
 * characters are covered.
 *
 * For text without newlines the whole string is returned regardless of
 * `charLength` — there are no line boundaries to respect.
 */
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

  // Snap back to the start of the current line when mid-line.
  let actualOffset = charOffset;
  if (actualOffset > 0 && text[actualOffset - 1] !== '\n') {
    const prevNewline = text.lastIndexOf('\n', actualOffset - 1);
    actualOffset = prevNewline === -1 ? 0 : prevNewline + 1;
  }

  // Collect complete lines until we have covered at least charLength chars.
  let endPos = actualOffset;
  let lineCount = 0;

  while (endPos < totalChars) {
    const nextNewline = text.indexOf('\n', endPos);
    if (nextNewline === -1) {
      // No more newlines — include the rest as a partial (no trailing \n).
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
