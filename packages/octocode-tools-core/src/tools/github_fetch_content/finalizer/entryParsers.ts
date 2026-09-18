import { fetchContinuation } from '../../../utils/file/contentPagination.js';
import type { FetchPagination } from '@octocodeai/octocode-core/extra-types';
import { classifyFileType } from '../../../utils/file/configFiles.js';
import type {
  FileContentNextMap,
  FileEntry,
  PartialFileContentQuery,
} from './types.js';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function readString(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

export function readNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

export function readStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const strings = value.filter(
    (item): item is string => typeof item === 'string'
  );
  return strings.length > 0 ? strings : undefined;
}

export function readPagination(value: unknown): FetchPagination | undefined {
  if (
    !isRecord(value) ||
    !['lines', 'bytes'].includes(String(value.chunkType)) ||
    typeof value.hasMore !== 'boolean'
  )
    return undefined;
  const fields = [
    'offset',
    'length',
    'limit',
    'totalLines',
    'totalBytes',
  ] as const;
  if (fields.some(field => readNumber(value[field]) === undefined))
    return undefined;
  return {
    chunkType: value.chunkType as 'lines' | 'bytes',
    offset: value.offset as number,
    length: value.length as number,
    limit: value.limit as number,
    totalLines: value.totalLines as number,
    totalBytes: value.totalBytes as number,
    hasMore: value.hasMore,
    ...(readNumber(value.nextOffset) !== undefined
      ? { nextOffset: value.nextOffset as number }
      : {}),
  };
}

export function readFileEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): FileEntry {
  const pagination = readPagination(data.pagination);
  const resolvedBranch = readString(data.resolvedBranch);
  const snapshotQuery = resolvedBranch
    ? { ...query, branch: resolvedBranch }
    : query;
  const continuation = fetchContinuation(
    snapshotQuery,
    pagination,
    'ghGetFileContent'
  );
  const next: FileContentNextMap = {
    ...(continuation ? { continue: continuation } : {}),
    ...(isRecord(data.next) ? (data.next as FileContentNextMap) : {}),
  };
  if (
    data.errorCode === 'contentSecurityLimit' &&
    (readNumber(data.totalLines) ?? 0) > 1 &&
    (query.startLine === undefined || query.startLine !== query.endLine)
  ) {
    const line = query.startLine ?? 1;
    next.readBoundedLines = {
      tool: 'ghGetFileContent',
      query: {
        owner: query.owner,
        repo: query.repo,
        path: query.path,
        ...(snapshotQuery.branch !== undefined
          ? { branch: snapshotQuery.branch }
          : {}),
        startLine: line,
        endLine: line,
        minify: 'none',
      },
      why: 'The selected view is too large to scan safely. Read one source line; this starts a different source-line view, not a continuation of the rejected view.',
      confidence: 'exact',
    };
  }
  const filePath = readString(data.path) ?? String(query.path ?? '');
  // Omit fileType entirely when classification is uncertain (undefined).
  const fileType = filePath ? classifyFileType(filePath) : undefined;
  return {
    path: filePath,
    content: typeof data.content === 'string' ? data.content : '',
    ...(Array.isArray(data.sourceLineRanges) && data.sourceLineRanges.length > 0
      ? {
          sourceLineRanges:
            data.sourceLineRanges as FileEntry['sourceLineRanges'],
        }
      : {}),
    ...(data.errorCode === 'contentSecurityLimit'
      ? {
          errorCode: 'contentSecurityLimit' as const,
          terminalLimit: true,
          partialReasons: ['security-selected-view-size-limit' as const],
        }
      : {}),
    ...(fileType ? { fileType } : {}),
    contentView:
      data.contentView === 'none' ||
      data.contentView === 'standard' ||
      data.contentView === 'symbols'
        ? data.contentView
        : undefined,
    totalLines: readNumber(data.totalLines),
    sourceChars: readNumber(data.sourceChars),
    sourceBytes: readNumber(data.sourceBytes),
    returnedChars: readNumber(data.returnedChars),
    returnedBytes: readNumber(data.returnedBytes),
    returnedLines: readNumber(data.returnedLines),
    selectedMatchCount: readNumber(data.selectedMatchCount),
    ...(isRecord(data.minifyFallback)
      ? { minifyFallback: data.minifyFallback as FileEntry['minifyFallback'] }
      : {}),
    ...(data.errorCode === 'noMatches'
      ? { errorCode: 'noMatches' as const }
      : {}),
    ...(data.errorCode === 'fullContentLimit'
      ? {
          errorCode: 'fullContentLimit' as const,
          partialReasons: ['full-content-size-limit' as const],
        }
      : {}),
    resolvedBranch: readString(data.resolvedBranch),
    ...(typeof data.commitSha === 'string' && data.commitSha.length === 40
      ? { commitSha: data.commitSha }
      : {}),
    pagination,
    ...(Object.keys(next).length > 0 ? { next } : {}),
    ...(data.isPartial === true ||
    (pagination as { hasMore?: boolean } | undefined)?.hasMore === true
      ? { isPartial: true }
      : {}),
    startLine: readNumber(data.startLine),
    endLine: readNumber(data.endLine),
    ...(Array.isArray(data.matchRanges) && data.matchRanges.length > 0
      ? {
          matchRanges: data.matchRanges as Array<{
            start: number;
            end: number;
          }>,
        }
      : {}),
    ...(Array.isArray(data.matchedLines) && data.matchedLines.length > 0
      ? { matchedLines: data.matchedLines as number[] }
      : {}),
    lastModified: readString(data.lastModified),
    lastModifiedBy: readString(data.lastModifiedBy),
    warnings: readStringArray(data.warnings),
    ...(data.matchNotFound === true ? { matchNotFound: true } : {}),
    searchedFor: readString(data.searchedFor),
    ...(data.cached === true ? { cached: true } : {}),
  };
}
