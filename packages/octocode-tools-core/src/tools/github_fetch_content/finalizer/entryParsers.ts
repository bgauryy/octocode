import type { PaginationInfo } from '../../../types/toolResults.js';
import { buildContinueCharsContinuation } from '../../../scheme/pagination.js';
import type {
  DirectoryEntry,
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

function readRequiredNumber(
  record: Record<string, unknown>,
  key: string
): number {
  return readNumber(record[key]) ?? 0;
}

function readDirectorySkipped(
  value: unknown
): DirectoryEntry['skipped'] | undefined {
  if (!isRecord(value)) return undefined;
  return {
    nonFile: readRequiredNumber(value, 'nonFile'),
    missingDownloadUrl: readRequiredNumber(value, 'missingDownloadUrl'),
    oversized: readRequiredNumber(value, 'oversized'),
    binary: readRequiredNumber(value, 'binary'),
    fileLimit: readRequiredNumber(value, 'fileLimit'),
    fetchFailed: readRequiredNumber(value, 'fetchFailed'),
    totalSizeLimit: readRequiredNumber(value, 'totalSizeLimit'),
    pathTraversal: readRequiredNumber(value, 'pathTraversal'),
  };
}

function readDirectoryLimits(
  value: unknown
): DirectoryEntry['limits'] | undefined {
  if (!isRecord(value)) return undefined;
  return {
    maxDirectoryFiles: readRequiredNumber(value, 'maxDirectoryFiles'),
    maxTotalSize: readRequiredNumber(value, 'maxTotalSize'),
    maxFileSize: readRequiredNumber(value, 'maxFileSize'),
  };
}

const OPTIONAL_PAGINATION_NUMERIC_FIELDS = [
  'charOffset',
  'charLength',
  'totalChars',
  'nextCharOffset',
  'nextBlockChar',
  'nextPage',
  'nextMatchPage',
  'filesPerPage',
  'totalFiles',
  'entriesPerPage',
  'totalEntries',
  'matchesPerPage',
  'totalMatches',
] as const satisfies ReadonlyArray<keyof PaginationInfo>;

export function readPagination(value: unknown): PaginationInfo | undefined {
  if (!isRecord(value)) return undefined;
  const { currentPage, totalPages, hasMore } = value;
  if (
    typeof currentPage !== 'number' ||
    typeof totalPages !== 'number' ||
    typeof hasMore !== 'boolean'
  ) {
    return undefined;
  }
  const result: PaginationInfo = { currentPage, totalPages, hasMore };
  for (const field of OPTIONAL_PAGINATION_NUMERIC_FIELDS) {
    const candidate = value[field];
    if (typeof candidate === 'number' && Number.isFinite(candidate)) {
      result[field] = candidate;
    }
  }
  return result;
}

function buildContinueChars(
  pagination: PaginationInfo | undefined,
  query: PartialFileContentQuery
): FileContentNextMap | undefined {
  return buildContinueCharsContinuation(
    'ghGetFileContent',
    {
      owner: query.owner,
      repo: query.repo,
      ...(query.branch !== undefined ? { branch: query.branch } : {}),
      path: query.path,
      ...(query.minify !== undefined ? { minify: query.minify } : {}),
    },
    pagination
  ) as FileContentNextMap | undefined;
}

// This was the ONLY fetch/search tool that could emit zero next-hints (a
// fully-read, non-paginated file has nothing left to continue). lspGetSemantics
// only resolves definitions/references against local files, not GitHub reads
// directly, so hand the agent the one-step bridge instead of a dead end.
function buildCloneForSemanticsHint(
  query: PartialFileContentQuery
): FileContentNextMap['cloneForSemantics'] {
  return {
    tool: 'ghCloneRepo',
    query: {
      owner: query.owner,
      repo: query.repo,
      ...(query.branch !== undefined ? { branch: query.branch } : {}),
      sparsePath: query.path,
    },
    why: 'lspGetSemantics (definitions/references) only works on local files — clone this path locally, then run localSearchCode or lspGetSemantics on it',
    confidence: 'exact',
  };
}

export function readFileEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): FileEntry {
  const pagination = readPagination(data.pagination);
  const next: FileContentNextMap = {
    ...buildContinueChars(pagination, query),
    cloneForSemantics: buildCloneForSemanticsHint(query),
  };
  return {
    path: readString(data.path) ?? String(query.path ?? ''),
    content: typeof data.content === 'string' ? data.content : '',
    localPath: readString(data.localPath),
    repoRoot: readString(data.repoRoot),
    ...(readNumber(data.fileSize) !== undefined
      ? { fileSize: readNumber(data.fileSize) }
      : {}),
    contentView:
      data.contentView === 'none' ||
      data.contentView === 'standard' ||
      data.contentView === 'symbols'
        ? data.contentView
        : undefined,
    totalLines: readNumber(data.totalLines),
    sourceChars: readNumber(data.sourceChars),
    sourceBytes: readNumber(data.sourceBytes),
    resolvedBranch: readString(data.resolvedBranch),
    pagination,
    next,
    ...(data.isPartial === true ? { isPartial: true } : {}),
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
    lastModified: readString(data.lastModified),
    lastModifiedBy: readString(data.lastModifiedBy),
    warnings: readStringArray(data.warnings),
    ...(data.matchNotFound === true ? { matchNotFound: true } : {}),
    searchedFor: readString(data.searchedFor),
    ...(data.cached === true ? { cached: true } : {}),
  };
}

export function readDirectoryEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): DirectoryEntry {
  const rawFiles = Array.isArray(data.files) ? data.files : [];
  const files = rawFiles.filter(isRecord).map(file => ({
    path: readString(file.path) ?? '',
    size: readNumber(file.size) ?? 0,
    type: readString(file.type) ?? 'file',
  }));

  const skipped = readDirectorySkipped(data.skipped);
  const hasSubdirectories =
    data.hasSubdirectories === true || (skipped ? skipped.nonFile > 0 : false);
  const skippedSummaryEntries = skipped
    ? Object.entries(skipped).filter(([, v]) => v > 0)
    : [];
  const skippedSummary =
    skippedSummaryEntries.length > 0
      ? Object.fromEntries(skippedSummaryEntries)
      : undefined;

  return {
    path: String(query.path ?? ''),
    localPath: readString(data.localPath) ?? '',
    repoRoot: readString(data.repoRoot),
    fileCount: readNumber(data.fileCount) ?? files.length,
    totalSize: readNumber(data.totalSize) ?? 0,
    complete: data.complete === true,
    verified: data.verified === true,
    ...(typeof data.commitSha === 'string' && data.commitSha.length === 40
      ? { commitSha: data.commitSha }
      : {}),
    ...(hasSubdirectories ? { hasSubdirectories: true } : {}),
    ...(skippedSummary ? { skippedSummary } : {}),
    directoryEntryCount: readNumber(data.directoryEntryCount),
    eligibleFileCount: readNumber(data.eligibleFileCount),
    savedFileCount: readNumber(data.savedFileCount),
    skipped: skipped,
    limits: readDirectoryLimits(data.limits),
    warnings: readStringArray(data.warnings),
    ...(files.length > 0 ? { files } : {}),
    ...(data.cached === true ? { cached: true } : {}),
    resolvedBranch: readString(data.resolvedBranch),
  };
}
