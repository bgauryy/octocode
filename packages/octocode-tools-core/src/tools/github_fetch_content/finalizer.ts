import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { BulkFinalizer } from '../../types/bulk.js';
import type {
  FlatQueryResult,
  PaginationInfo,
} from '../../types/toolResults.js';
import {
  collectFlatErrors,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type { GitHubFetchContentOutputLocal } from './scheme.js';
import type { WithOptionalMeta } from '../../types/execution.js';

type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

type FileEntry = {
  path: string;
  content: string;
  localPath?: string;
  repoRoot?: string;
  fileSize?: number;
  contentView?: 'none' | 'standard' | 'symbols';
  isSkeleton?: boolean;
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  resolvedBranch?: string;
  pagination?: PaginationInfo;
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
  cached?: boolean;
  next?: FileContentNextMap;
};

type FileContentNextMap = {
  continueChars?: {
    tool: 'ghGetFileContent';
    query: Record<string, unknown>;
  };
};

type DirectoryEntry = {
  path: string;
  localPath: string;
  repoRoot?: string;
  fileCount: number;
  totalSize: number;
  complete?: boolean;
  verified?: boolean;
  commitSha?: string;
  hasSubdirectories?: boolean;
  skippedSummary?: Record<string, number>;
  directoryEntryCount?: number;
  eligibleFileCount?: number;
  savedFileCount?: number;
  skipped?: {
    nonFile: number;
    missingDownloadUrl: number;
    oversized: number;
    binary: number;
    fileLimit: number;
    fetchFailed: number;
    totalSizeLimit: number;
    pathTraversal: number;
  };
  limits?: {
    maxDirectoryFiles: number;
    maxTotalSize: number;
    maxFileSize: number;
  };
  warnings?: string[];
  files?: Array<{ path: string; size: number; type: string }>;
  cached?: boolean;
  resolvedBranch?: string;
};

type RepoGroup = {
  id: string;
  owner: string;
  repo: string;
  files?: FileEntry[];
  directories?: DirectoryEntry[];
  data?: RepoGroupData;
};

type RepoGroupData = {
  owner: string;
  repo: string;
  files?: FileEntry[];
  directories?: DirectoryEntry[];
};

type FileContentResponse = GitHubFetchContentOutputLocal;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readString(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function readNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

function readStringArray(value: unknown): string[] | undefined {
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

function readPagination(value: unknown): PaginationInfo | undefined {
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

function groupId(owner: string, repo: string): string {
  return `${owner}/${repo}`;
}

function ensureGroup(
  groups: Map<string, RepoGroup>,
  owner: string,
  repo: string
): RepoGroup {
  const id = groupId(owner, repo);
  const existing = groups.get(id);
  if (existing) return existing;
  const created: RepoGroup = { id, owner, repo };
  groups.set(id, created);
  return created;
}

function buildContinueChars(
  pagination: PaginationInfo | undefined,
  query: PartialFileContentQuery
): FileContentNextMap | undefined {
  if (
    !pagination ||
    !pagination.hasMore ||
    pagination.nextCharOffset === undefined
  ) {
    return undefined;
  }
  // Same `next.continueChars` shape convention as localSearchCode/localGetFileContent;
  // built from the data already present so the agent can fetch the next page.
  return {
    continueChars: {
      tool: 'ghGetFileContent',
      query: {
        owner: query.owner,
        repo: query.repo,
        ...(query.branch !== undefined ? { branch: query.branch } : {}),
        path: query.path,
        charOffset: pagination.nextCharOffset,
        ...(pagination.charLength !== undefined
          ? { charLength: pagination.charLength }
          : {}),
        ...(query.minify !== undefined ? { minify: query.minify } : {}),
      },
    },
  };
}

function readFileEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): FileEntry {
  const pagination = readPagination(data.pagination);
  const next = buildContinueChars(pagination, query);
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
    ...(data.isSkeleton === true ? { isSkeleton: true } : {}),
    totalLines: readNumber(data.totalLines),
    sourceChars: readNumber(data.sourceChars),
    sourceBytes: readNumber(data.sourceBytes),
    resolvedBranch: readString(data.resolvedBranch),
    pagination,
    ...(next ? { next } : {}),
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

function readDirectoryEntry(
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

function buildGroups(
  results: readonly FlatQueryResult[],
  queries: readonly PartialFileContentQuery[]
): Array<{ id: string; data: RepoGroupData }> {
  const groups = new Map<string, RepoGroup>();

  results.forEach((result, index) => {
    if (result.status === 'error') return;
    const query = queries[index];
    if (!query) return;
    const owner = String(query.owner ?? '');
    const repo = String(query.repo ?? '');
    if (!owner || !repo) return;

    const group = ensureGroup(groups, owner, repo);
    const data = result.data;

    if (query.type === 'directory') {
      const directories = group.directories ?? [];
      directories.push(readDirectoryEntry(data, query));
      group.directories = directories;
      return;
    }

    const files = group.files ?? [];
    files.push(readFileEntry(data, query));
    group.files = files;
  });

  return Array.from(groups.values()).map(group => {
    const data: RepoGroupData = {
      owner: group.owner,
      repo: group.repo,
      ...(group.files ? { files: group.files } : {}),
      ...(group.directories ? { directories: group.directories } : {}),
    };
    // Emit only { id, data } — the canonical row shape. owner/repo/files/
    // directories live ONLY under data (previously also mirrored flat at the
    // top level, byte-identical, which doubled file-content payloads).
    return { id: group.id, data };
  });
}

function collectFileErrors(
  results: readonly FlatQueryResult[],
  queries: readonly PartialFileContentQuery[]
): FileContentResponse['errors'] {
  const base = collectFlatErrors(results);
  return base.map(error => {
    const index = results.findIndex(result => result.id === error.id);
    const query = index >= 0 ? queries[index] : undefined;
    return {
      id: error.id,
      owner: query?.owner,
      repo: query?.repo,
      path: query?.path ? String(query.path) : undefined,
      error: error.error,
    };
  });
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results }) => {
    const groups = buildGroups(results, queries);

    const errors = collectFileErrors(results, queries);
    const responseData: FileContentResponse = { results: groups };

    if (errors && errors.length > 0) responseData.errors = errors;

    return formatFinalizedResponse<GitHubFetchContentOutputLocal>(
      responseData,
      [
        'results',
        'id',
        'owner',
        'repo',
        'files',
        'directories',
        'path',
        'content',
        'totalLines',
        'startLine',
        'endLine',
        'isPartial',
        'pagination',
        'errors',
      ],
      groups.length === 0 && Boolean(errors && errors.length > 0)
    );
  };
}
