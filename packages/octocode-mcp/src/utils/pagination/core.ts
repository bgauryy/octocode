/**
 * Core pagination utilities
 * Shared pagination logic for content pagination with proper byte/character handling
 */

import type { PaginationInfo } from '../../types/toolResults.js';
import type {
  PaginationMetadata,
  ApplyPaginationOptions,
  SliceByCharResult,
} from './types.js';
import { byteToCharIndex, charToByteIndex } from '../file/byteOffset.js';

/**
 * Apply pagination to content based on offset and length
 * Supports both character offsets (default) and byte offsets (for GitHub API compatibility)
 *
 * Returns both byte-based and character-based offsets in the result:
 * - Use byteOffset/nextByteOffset for GitHub API or byte-level operations
 * - Use charOffset/nextCharOffset for JavaScript string operations
 *
 * @param content - The content to paginate
 * @param offset - The starting offset (interpreted based on mode)
 * @param length - The page size (interpreted based on mode)
 * @param options - Pagination options including mode ('bytes' or 'characters')
 */
export function applyPagination(
  content: string,
  offset: number = 0,
  length?: number,
  options: ApplyPaginationOptions = {}
): PaginationMetadata {
  const mode = options.mode ?? 'characters';
  const totalChars = content.length;
  const totalBytes = Buffer.byteLength(content, 'utf-8');

  // No pagination requested - return full content
  if (length === undefined) {
    return {
      paginatedContent: content,
      // Byte fields
      byteOffset: 0,
      byteLength: totalBytes,
      totalBytes,
      nextByteOffset: undefined,
      // Character fields
      charOffset: 0,
      charLength: totalChars,
      totalChars,
      nextCharOffset: undefined,
      // Common fields
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
    // Byte mode: offset and length are in bytes
    // Convert byte positions to character positions first to avoid mid-character slicing
    // This ensures we always slice on UTF-8 character boundaries
    const requestedStartByte = Math.min(offset, totalBytes);
    const requestedEndByte = Math.min(requestedStartByte + length, totalBytes);

    // Convert to character indices (this aligns to character boundaries)
    startCharPos = byteToCharIndex(content, requestedStartByte);
    endCharPos = byteToCharIndex(content, requestedEndByte);

    // Extract content using safe character positions
    paginatedContent = content.substring(startCharPos, endCharPos);

    // Calculate actual byte positions from the aligned character positions
    startBytePos = charToByteIndex(content, startCharPos);
    endBytePos = charToByteIndex(content, endCharPos);

    hasMore = endBytePos < totalBytes;
    // Use actualOffset for page calculation if provided
    const pageOffset = options.actualOffset ?? requestedStartByte;
    currentPage = Math.floor(pageOffset / length) + 1;
    totalPages = Math.ceil(totalBytes / length);
  } else {
    // Character mode: offset and length are in characters
    startCharPos = Math.min(offset, totalChars);
    endCharPos = Math.min(startCharPos + length, totalChars);

    // Extract content using character positions
    paginatedContent = content.substring(startCharPos, endCharPos);

    // Calculate byte positions from character positions
    startBytePos = charToByteIndex(content, startCharPos);
    endBytePos = charToByteIndex(content, endCharPos);

    hasMore = endCharPos < totalChars;
    // Use actualOffset for page calculation if provided
    const pageOffset = options.actualOffset ?? startCharPos;
    currentPage = Math.floor(pageOffset / length) + 1;
    totalPages = Math.ceil(totalChars / length);
  }

  return {
    paginatedContent,
    // Byte fields - actual byte values
    byteOffset: startBytePos,
    byteLength: endBytePos - startBytePos,
    totalBytes,
    nextByteOffset: hasMore ? endBytePos : undefined,
    // Character fields - actual character values
    charOffset: startCharPos,
    charLength: paginatedContent.length,
    totalChars,
    nextCharOffset: hasMore ? endCharPos : undefined,
    // Common fields
    hasMore,
    estimatedTokens: Math.ceil(paginatedContent.length / 4),
    currentPage,
    totalPages,
  };
}

/**
 * Serialize data for pagination (convert to JSON string)
 */
export function serializeForPagination(
  data: unknown,
  pretty: boolean = false
): string {
  return pretty ? JSON.stringify(data, null, 2) : JSON.stringify(data);
}

/**
 * Slice text by character count while respecting line boundaries
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
      nextOffset: totalChars,
      lineCount: 0,
      totalChars,
    };
  }

  const lines: number[] = [0];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\n') {
      lines.push(i + 1); // Start of next line
    }
  }

  let startLineIdx = 0;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i]! <= charOffset) {
      startLineIdx = i;
    } else {
      break;
    }
  }

  const actualOffset = lines[startLineIdx]!;

  let endPos = Math.min(actualOffset + charLength, totalChars);
  let endLineIdx = startLineIdx;

  for (let i = startLineIdx; i < lines.length; i++) {
    if (lines[i]! < endPos) {
      endLineIdx = i;
    } else {
      break;
    }
  }

  if (endLineIdx < lines.length - 1 && endPos < lines[endLineIdx + 1]!) {
    endPos = lines[endLineIdx + 1]!;
  } else if (endLineIdx === lines.length - 1 && endPos < totalChars) {
    endPos = totalChars;
  }

  const sliced = text.substring(actualOffset, endPos);
  const hasMore = endPos < totalChars;
  const lineCount = sliced.split('\n').length - 1; // Count newlines

  return {
    sliced,
    actualOffset,
    actualLength: sliced.length,
    hasMore,
    nextOffset: hasMore ? endPos : undefined,
    lineCount,
    totalChars,
  };
}

/**
 * Create PaginationInfo from PaginationMetadata
 * Includes both byte and character offset fields
 */
export function createPaginationInfo(
  metadata: PaginationMetadata
): PaginationInfo {
  return {
    currentPage: metadata.currentPage,
    totalPages: metadata.totalPages,
    hasMore: metadata.hasMore,
    // Byte fields
    byteOffset: metadata.byteOffset,
    byteLength: metadata.byteLength,
    totalBytes: metadata.totalBytes,
    // Character fields
    charOffset: metadata.charOffset,
    charLength: metadata.charLength,
    totalChars: metadata.totalChars,
  };
}
