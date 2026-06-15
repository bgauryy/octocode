import type { PaginationInfo } from '../../types/toolResults.js';
import type { PaginationMetadata, ApplyPaginationOptions } from './types.js';
import { byteToCharIndex, charToByteIndex } from '../file/byteOffset.js';

export function applyPagination(
  content: string,
  offset: number = 0,
  length?: number,
  options: ApplyPaginationOptions = {}
): PaginationMetadata {
  const mode = options.mode ?? 'characters';
  const totalChars = content.length;
  const totalBytes = Buffer.byteLength(content, 'utf-8');

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

    paginatedContent = content.substring(startCharPos, endCharPos);

    startBytePos = charToByteIndex(content, startCharPos);
    endBytePos = charToByteIndex(content, endCharPos);

    hasMore = endBytePos < totalBytes;
    const pageOffset = options.actualOffset ?? requestedStartByte;
    currentPage = Math.floor(pageOffset / length) + 1;
    totalPages = Math.ceil(totalBytes / length);
  } else {
    startCharPos = Math.min(offset, totalChars);
    endCharPos = Math.min(startCharPos + length, totalChars);

    paginatedContent = content.substring(startCharPos, endCharPos);

    startBytePos = charToByteIndex(content, startCharPos);
    endBytePos = charToByteIndex(content, endCharPos);

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
