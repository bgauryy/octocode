import type { FileContentQuery } from '@octocodeai/octocode-core';
import type { BulkFinalizer } from '../../types/bulk.js';
import type {
  FlatQueryResult,
  PaginationInfo,
} from '../../types/toolResults.js';
import {
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  paginateGroupsWithNestedItemEscape,
  type CharPagination,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type {
  GitHubFetchContentOutputLocal,
  GroupedToolWarning,
} from '../../scheme/remoteSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import { tsvFormat } from '../../utils/response/tsvFormat.js';
import { getTsvProjection } from '../../utils/response/tsvColumns.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

type FileEntry = {
  path: string;
  content: string;
  totalLines?: number;
  resolvedBranch?: string;
  pagination?: PaginationInfo;
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
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

// Structurally identical to `GitHubFetchContentOutputLocal` now that the
// schema's pagination fields are tightened (file pagination →
// PaginationInfoSchema; responsePagination → CharPaginationSchema).  Kept as
// a local alias for readability; no cast needed at formatFinalizedResponse.
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

const OPTIONAL_PAGINATION_NUMERIC_FIELDS = [
  'byteOffset',
  'byteLength',
  'totalBytes',
  'charOffset',
  'charLength',
  'totalChars',
  'filesPerPage',
  'totalFiles',
  'entriesPerPage',
  'totalEntries',
  'matchesPerPage',
  'totalMatches',
] as const satisfies ReadonlyArray<keyof PaginationInfo>;

/**
 * Narrow an opaque per-query data slot into a `PaginationInfo`.
 *
 * The bulk runner types `FlatQueryResult.data` as `Record<string, unknown>`
 * so each tool is free to define its own per-query payload.  We validate
 * the three required fields, then copy the optional numeric counters
 * field-by-field so the returned value is structurally typed without an
 * `as unknown as` boundary cast.
 */
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
    totalLines: readNumber(data.totalLines),
    resolvedBranch: readString(data.resolvedBranch),
    pagination: readPagination(data.pagination),
    ...(data.isPartial === true ? { isPartial: true } : {}),
    startLine: readNumber(data.startLine),
    endLine: readNumber(data.endLine),
    lastModified: readString(data.lastModified),
    lastModifiedBy: readString(data.lastModifiedBy),
    warnings: readStringArray(data.warnings),
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

function getGroupItems(
  group: RepoGroup
): readonly (FileEntry | DirectoryEntry)[] {
  return [...(group.files ?? []), ...(group.directories ?? [])];
}

function setGroupItems(
  group: RepoGroup,
  items: Array<FileEntry | DirectoryEntry>
): RepoGroup {
  const files = items.filter((item): item is FileEntry => 'content' in item);
  const directories = items.filter(
    (item): item is DirectoryEntry => 'localPath' in item
  );
  return {
    ...group,
    ...(files.length > 0 ? { files } : { files: undefined }),
    ...(directories.length > 0 ? { directories } : { directories: undefined }),
  };
}

function makeTruncator(warnings: GroupedToolWarning[]) {
  return (
    item: FileEntry | DirectoryEntry,
    charLength: number,
    group: RepoGroup
  ): FileEntry | DirectoryEntry => {
    if (!('content' in item)) return item;
    const marker = '\n… [truncated by responseCharLength]';
    const budget = Math.max(
      0,
      charLength - item.path.length - marker.length - 128
    );
    if (item.content.length <= budget) return item;
    warnings.push({
      kind: 'content-truncated',
      groupId: group.id,
      path: item.path,
      fullContentLength: item.content.length,
      truncatedAt: budget,
      recovery:
        "Re-query with larger responseCharLength, or use this file's startLine/endLine to fetch the rest precisely.",
    });
    return {
      ...item,
      content: `${item.content.slice(0, budget)}${marker}`,
      warnings: dedupeHints([
        ...(item.warnings ?? []),
        'Content truncated by responseCharLength; see top-level warnings[] for structured signal.',
      ]),
    };
  };
}

function buildRuntimeHints(
  groups: readonly RepoGroup[],
  responsePagination?: CharPagination
): string[] {
  const hints: string[] = [];

  for (const group of groups) {
    for (const file of group.files ?? []) {
      if (
        file.pagination?.hasMore &&
        typeof file.pagination.charOffset === 'number'
      ) {
        const currentLength = file.pagination.charLength ?? 0;
        hints.push(
          `Use charOffset=${file.pagination.charOffset + currentLength} for ${group.id}:${file.path} to continue this file.`
        );
      }
      // Partial line-range continuation is intentionally NOT hinted at the
      // top level — the agent already has isPartial/endLine/totalLines on
      // each file entry, so duplicating the math here is pure redundancy.
    }

    for (const directory of group.directories ?? []) {
      hints.push(
        `Directory ${group.id}:${directory.path} saved to localPath; use local tools on that path.`
      );
      if (directory.cached)
        hints.push(
          `Directory ${group.id}:${directory.path} served from cache.`
        );
    }
  }

  if (responsePagination?.hasMore) {
    hints.push(
      `Use responseCharOffset=${responsePagination.charOffset + responsePagination.charLength} to continue this paginated bulk response.`
    );
  }

  return dedupeHints(hints);
}

function errorHints(error: string, status?: number): string[] | undefined {
  const lower = error.toLowerCase();
  if (status === 404 || lower.includes('not found') || lower.includes('404')) {
    return [
      'Verify owner/repo/path/branch.',
      'Use githubViewRepoStructure to confirm the path.',
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
      hints: errorHints(error.error, error.status),
    };
  });
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results, config }) => {
    let groups = buildGroups(results, queries);
    let responsePagination: CharPagination | undefined;
    const warnings: GroupedToolWarning[] = [];

    if (
      groups.length > 0 &&
      (config.responseCharLength !== undefined ||
        config.responseCharOffset !== undefined)
    ) {
      const sliced = paginateGroupsWithNestedItemEscape({
        groups,
        getItems: getGroupItems,
        setItems: setGroupItems,
        charOffset: config.responseCharOffset ?? 0,
        charLength: config.responseCharLength ?? Number.MAX_SAFE_INTEGER,
        truncateOversizedItem: makeTruncator(warnings),
      });
      groups = sliced.groups;
      responsePagination = sliced.pagination;
    }

    const errors = collectFileErrors(results, queries);
    const hints = buildRuntimeHints(groups, responsePagination);
    const responseData: FileContentResponse = { results: groups };

    if (responsePagination)
      responseData.responsePagination = responsePagination;
    if (hints.length > 0) responseData.hints = hints;
    if (warnings.length > 0) responseData.warnings = warnings;
    if (errors && errors.length > 0) responseData.errors = errors;

    if (config.format === 'tsv') {
      const projection = getTsvProjection(
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT
      );
      if (projection) {
        responseData.format = 'tsv';
        responseData.columns = [...projection.columns];
        responseData.rows = tsvFormat(
          projection.columns,
          projection.toRows({ results: groups })
        );
      }
    }

    return formatFinalizedResponse<GitHubFetchContentOutputLocal>(
      responseData,
      [
        'format',
        'columns',
        'rows',
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
        'responsePagination',
        'hints',
        'warnings',
        'errors',
      ],
      groups.length === 0 && Boolean(errors && errors.length > 0)
    );
  };
}
