import {
  applyPagination,
  createPaginationInfo,
} from '../utils/pagination/core.js';
import type { PaginationInfo } from '../types.js';
import type { FileContentQuery } from './providerQueries.js';

const DEFAULT_CONTENT_PAGE_SIZE = 20_000;

interface ExtractedFileContent {
  content: string;
  hints?: string[];
  pagination?: PaginationInfo;
  isPartial: boolean;
  totalLines: number;
  startLine?: number;
  endLine?: number;
}

function extractMatchingContent(
  content: string,
  query: FileContentQuery
): ExtractedFileContent {
  const lines = content.split('\n');
  const totalLines = lines.length;
  const matchString = query.matchString;
  const contextLines = query.matchStringContextLines ?? 5;

  if (!matchString) {
    return {
      content,
      isPartial: false,
      totalLines,
    };
  }

  const search = matchString.toLowerCase();
  const matchingLines: number[] = [];

  for (let index = 0; index < lines.length; index++) {
    if (lines[index]?.toLowerCase().includes(search)) {
      matchingLines.push(index + 1);
    }
  }

  if (matchingLines.length === 0) {
    return {
      content: '',
      hints: [
        `Pattern "${matchString}" not found in file. Try broader search or verify path.`,
      ],
      isPartial: false,
      totalLines,
    };
  }

  const firstMatch = matchingLines[0]!;
  const startLine = Math.max(1, firstMatch - contextLines);
  const endLine = Math.min(totalLines, firstMatch + contextLines);

  return {
    content: lines.slice(startLine - 1, endLine).join('\n'),
    isPartial: true,
    totalLines,
    startLine,
    endLine,
  };
}

function extractLineRangeContent(
  content: string,
  query: FileContentQuery
): ExtractedFileContent {
  const lines = content.split('\n');
  const totalLines = lines.length;
  const startLine = query.startLine ?? 1;
  const endLine = query.endLine ?? totalLines;

  if (startLine < 1 || startLine > totalLines || endLine < startLine) {
    return {
      content,
      isPartial: false,
      totalLines,
    };
  }

  const clampedStartLine = Math.max(1, startLine);
  const clampedEndLine = Math.min(totalLines, endLine);

  return {
    content: lines.slice(clampedStartLine - 1, clampedEndLine).join('\n'),
    isPartial: true,
    totalLines,
    startLine: clampedStartLine,
    endLine: clampedEndLine,
  };
}

function applyCharacterPagination(
  extracted: ExtractedFileContent,
  query: FileContentQuery
): ExtractedFileContent {
  const requestedLength =
    query.charLength ??
    (query.charOffset !== undefined ||
    extracted.content.length > DEFAULT_CONTENT_PAGE_SIZE
      ? DEFAULT_CONTENT_PAGE_SIZE
      : undefined);

  if (requestedLength === undefined) {
    return extracted;
  }

  const paginationMetadata = applyPagination(
    extracted.content,
    query.charOffset ?? 0,
    requestedLength,
    { mode: 'characters' }
  );

  return {
    ...extracted,
    content: paginationMetadata.paginatedContent,
    pagination: createPaginationInfo(paginationMetadata),
  };
}

export function extractFileContent(
  content: string,
  query: FileContentQuery
): ExtractedFileContent {
  let extracted: ExtractedFileContent;

  if (!query.fullContent && query.matchString) {
    extracted = extractMatchingContent(content, query);
  } else if (
    !query.fullContent &&
    (query.startLine !== undefined || query.endLine !== undefined)
  ) {
    extracted = extractLineRangeContent(content, query);
  } else {
    extracted = {
      content,
      isPartial: false,
      totalLines: content.split('\n').length,
    };
  }

  return applyCharacterPagination(extracted, query);
}
