import type { PaginationInfo } from '../../types/toolResults.js';
import type {
  PaginationMetadata,
  ApplyPaginationOptions,
  SliceByCharResult,
} from './types.js';
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
      lines.push(i + 1);
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
  const lineCount = sliced.split('\n').length - 1;

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

export function createPaginationInfo(
  metadata: PaginationMetadata
): PaginationInfo {
  return {
    currentPage: metadata.currentPage,
    totalPages: metadata.totalPages,
    hasMore: metadata.hasMore,
    byteOffset: metadata.byteOffset,
    byteLength: metadata.byteLength,
    totalBytes: metadata.totalBytes,
    charOffset: metadata.charOffset,
    charLength: metadata.charLength,
    totalChars: metadata.totalChars,
  };
}
