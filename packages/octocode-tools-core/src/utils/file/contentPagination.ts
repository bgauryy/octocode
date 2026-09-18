import { matchContext } from './matchContext.js';
import type { FetchPagination } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from '@octocodeai/octocode-core/schema';
import { buildNextPageContinuation } from '../../scheme/pagination.js';
import { countLines } from '../core/lines.js';

export type FetchTool = 'localFetch' | 'ghGetFileContent';
export type FetchChunkQuery = Pick<
  FetchContentQuery,
  | 'chunkType'
  | 'offset'
  | 'limit'
  | 'fullContent'
  | 'matchString'
  | 'contextLines'
  | 'contextBytes'
  | 'startLine'
  | 'endLine'
>;
const DEFAULT_BYTE_LIMIT = 16384;

export interface ContentWindow {
  windowedContent: string;
  pagination: FetchPagination;
  firstViewLine: number;
  lastViewLine: number;
  next?: Record<string, ReturnType<typeof buildNextPageContinuation>>;
}

/** Offsets address the complete sanitized view, including its original line endings. */
export async function paginateContentWindow(
  content: string,
  query: FetchChunkQuery,
  tool: FetchTool
): Promise<ContentWindow> {
  const bytes = Buffer.from(content, 'utf8');
  const lines = content.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  let chunkType = query.chunkType ?? 'lines';
  // An explicit selection is already scoped by the caller. Fill its byte budget
  // rather than making a small requested region take several 100-line round trips.
  const selected =
    query.matchString !== undefined ||
    (query.startLine !== undefined && query.endLine !== undefined);
  const defaultLines = selected
    ? Math.max(1, Math.min(lines.length, 50000))
    : 100;
  let limit =
    query.limit ?? (chunkType === 'lines' ? defaultLines : DEFAULT_BYTE_LIMIT);
  let offset = query.offset ?? 0;
  let start = 0;
  let end = 0;
  let length = 0;
  let nextOffset = offset;
  if (query.fullContent) {
    chunkType = 'bytes';
    limit = bytes.length;
    end = bytes.length;
    length = bytes.length;
  } else if (chunkType === 'lines') {
    offset = Math.min(offset, lines.length);
    start = Buffer.byteLength(lines.slice(0, offset).join(''));
    end = start;
    while (offset + length < lines.length && length < limit) {
      const lineBytes = Buffer.byteLength(lines[offset + length]!);
      if (end - start + lineBytes > DEFAULT_BYTE_LIMIT) break;
      end += lineBytes;
      length++;
    }
    nextOffset = offset + length;
    if (length === 0 && start < bytes.length) {
      chunkType = 'bytes';
      offset = start;
      limit = DEFAULT_BYTE_LIMIT;
    }
  }
  if (!query.fullContent && chunkType === 'bytes') {
    offset = Math.min(offset, bytes.length);
    if (offset < bytes.length && (bytes[offset]! & 0xc0) === 0x80) {
      throw new Error('offset must be on a UTF-8 code point boundary');
    }
    start = offset;
    end = Math.min(start + limit, bytes.length);
    while (end < bytes.length && (bytes[end]! & 0xc0) === 0x80) end++;
    length = end - start;
    nextOffset = end;
  }
  const hasMore = end < bytes.length;
  const pagination: FetchPagination = {
    chunkType,
    offset,
    length,
    limit,
    totalLines: lines.length,
    totalBytes: bytes.length,
    hasMore,
    ...(hasMore ? { nextOffset } : {}),
  };
  const prefix = bytes.subarray(0, start).toString('utf8');
  const windowedContent = bytes.subarray(start, end).toString('utf8');
  const firstViewLine = prefix.split('\n').length;
  const continuation = fetchContinuation(query, pagination, tool);
  return {
    windowedContent,
    pagination,
    firstViewLine,
    lastViewLine: firstViewLine + countLines(windowedContent) - 1,
    ...(continuation ? { next: { continue: continuation } } : {}),
  };
}

export function pageFields(window: ContentWindow) {
  return {
    content: window.windowedContent,
    returnedChars: window.windowedContent.length,
    returnedBytes: Buffer.byteLength(window.windowedContent),
    returnedLines: countLines(window.windowedContent),
    pagination: window.pagination,
    ...(window.pagination.hasMore ? { isPartial: true } : {}),
    ...(window.next ? { next: window.next } : {}),
  };
}

export function fullContentLimit(
  query: FetchChunkQuery & { path: string },
  text: string,
  totalLines: number,
  tool: FetchTool
) {
  if (!query.fullContent || Buffer.byteLength(text) <= 50000) return undefined;
  const { fullContent: _fullContent, ...selection } = query;
  return {
    path: query.path,
    status: 'error' as const,
    errorCode: 'fullContentLimit',
    error:
      'The complete view exceeds 50000 bytes. Follow next.continue to read the same view in bounded chunks.',
    totalLines,
    isPartial: true,
    partialReasons: ['full-content-size-limit'],
    next: {
      continue: buildNextPageContinuation(tool, {
        ...selection,
        chunkType: 'lines',
        offset: 0,
        limit: 100,
      }),
    },
  };
}

export function fetchContinuation(
  query: FetchChunkQuery,
  pagination: FetchPagination | undefined,
  tool: FetchTool
) {
  if (!pagination?.hasMore || pagination.nextOffset === undefined)
    return undefined;
  return buildNextPageContinuation(
    tool,
    {
      ...query,
      ...(query.matchString !== undefined ? matchContext(query) : {}),
      chunkType: pagination.chunkType,
      offset: pagination.nextOffset,
      limit: pagination.limit,
    },
    'Continue the same selected view using the returned units and offset.'
  );
}
