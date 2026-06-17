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
  dedupeHints,
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
};

type DirectoryEntry = {
  path: string;
  localPath: string;
  fileCount: number;
  totalSize: number;
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

function collectPeerHints(results: readonly FlatQueryResult[]): string[] {
  return dedupeHints(
    results.flatMap(result => {
      const raw = result.data.hints;
      return Array.isArray(raw)
        ? raw.filter((hint): hint is string => typeof hint === 'string')
        : [];
    })
  );
}

const OPTIONAL_PAGINATION_NUMERIC_FIELDS = [
  'charOffset',
  'charLength',
  'totalChars',
  'nextCharOffset',
  'nextBlockChar',
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

function readFileEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): FileEntry {
  return {
    path: readString(data.path) ?? String(query.path ?? ''),
    content: typeof data.content === 'string' ? data.content : '',
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
    pagination: readPagination(data.pagination),
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
  };
}

function formatContentPageHint(groupId: string, file: FileEntry): string {
  const pagination = file.pagination;
  const currentOffset = pagination?.charOffset ?? 0;
  const currentLength = pagination?.charLength ?? 0;
  const nextOffset = currentOffset + currentLength;
  const page =
    pagination?.currentPage && pagination.totalPages
      ? `Page ${pagination.currentPage}/${pagination.totalPages}`
      : 'Content page';
  const range =
    typeof pagination?.totalChars === 'number' && currentLength > 0
      ? ` (chars ${currentOffset + 1}-${nextOffset} of ${pagination.totalChars})`
      : '';
  return `${page}${range}. Next: charOffset=${nextOffset} for ${groupId}:${file.path}`;
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

  return {
    path: String(query.path ?? ''),
    localPath: readString(data.localPath) ?? '',
    fileCount: readNumber(data.fileCount) ?? files.length,
    totalSize: readNumber(data.totalSize) ?? 0,
    ...(files.length > 0 ? { files } : {}),
    ...(data.cached === true ? { cached: true } : {}),
    resolvedBranch: readString(data.resolvedBranch),
  };
}

function buildGroups(
  results: readonly FlatQueryResult[],
  queries: readonly PartialFileContentQuery[]
): RepoGroup[] {
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

  return Array.from(groups.values());
}

function buildRuntimeHints(groups: readonly RepoGroup[]): string[] {
  const hints: string[] = [];

  for (const group of groups) {
    for (const file of group.files ?? []) {
      if (
        file.pagination?.hasMore &&
        typeof file.pagination.charOffset === 'number'
      ) {
        const currentLength = file.pagination.charLength ?? 0;
        const nextOffset = file.pagination.charOffset + currentLength;
        const nextBlockChar = file.pagination.nextBlockChar;

        if (typeof nextBlockChar === 'number') {
          const extendBy = nextBlockChar - nextOffset;
          hints.push(
            `${formatContentPageHint(group.id, file)}. Page cut mid-block at char ${nextOffset}. ` +
              `Next top-level definition starts at char ${nextBlockChar}. ` +
              `Re-request with charLength=${currentLength + extendBy} to extend this page to the next boundary, ` +
              `or use charOffset=${nextOffset} to continue page-by-page.`
          );
        } else {
          hints.push(formatContentPageHint(group.id, file));
        }
      }
      if (
        file.isPartial &&
        !file.matchRanges?.length &&
        typeof file.endLine === 'number' &&
        typeof file.totalLines === 'number' &&
        file.endLine < file.totalLines
      ) {
        hints.push(
          `File content is partial (lines ${file.startLine ?? 1}–${file.endLine} of ${file.totalLines}). Use startLine=${file.endLine + 1} to read the next section of ${group.id}:${file.path}.`
        );
      }
    }

    for (const directory of group.directories ?? []) {
      if (directory.cached)
        hints.push(
          `Directory ${group.id}:${directory.path} served from cache.`
        );
    }
  }

  return dedupeHints(hints);
}

function errorHints(error: string, status?: number): string[] | undefined {
  const lower = error.toLowerCase();
  if (status === 404 || lower.includes('not found') || lower.includes('404')) {
    return [
      'Verify owner/repo/path/branch.',
      'Use ghViewRepoStructure to confirm the path.',
    ];
  }
  if (status === 403 || lower.includes('forbidden') || lower.includes('403')) {
    return ['Check token permissions or repository visibility.'];
  }
  if (status === 429 || lower.includes('rate limit')) {
    return ['Retry after reset or authenticate with a higher-limit token.'];
  }
  return undefined;
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
      hints: errorHints(error.error),
    };
  });
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results, config }) => {
    const groups = buildGroups(results, queries);

    const errors = collectFileErrors(results, queries);
    const hints = dedupeHints([
      ...(config.peerHints ? collectPeerHints(results) : []),
      ...buildRuntimeHints(groups),
    ]);
    const responseData: FileContentResponse = { results: groups };

    if (hints.length > 0) responseData.hints = hints;
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
        'hints',
        'errors',
      ],
      groups.length === 0 && Boolean(errors && errors.length > 0)
    );
  };
}
