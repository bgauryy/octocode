import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../../types/execution.js';
import type { ViewStructureQuery as LocalViewStructureQuery } from './scheme.js';
import { ToolErrors } from '../../errors/errorFactories.js';
import {
  applyEntryFilters,
  toEntryObject,
  toGroupedLists,
  type DirectoryEntry,
} from './structureFilters.js';
import {
  buildWalkWarnings,
  paginateEntries,
  summarizeEntries,
} from './structureResponse.js';
import { LOCAL_MAX_LIMIT, MAX_PAGE_NUMBER } from '../../config.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { contextUtils } from '../../utils/contextUtils.js';
import type { FileSystemEntry } from '@octocodeai/octocode-engine';
import { buildNextPageContinuation } from '../../scheme/pagination.js';
import { buildViewStructureNextMap } from './viewStructureNext.js';

type ViewStructureQuery = WithOptionalMeta<LocalViewStructureQuery>;

const DEFAULT_VIEW_STRUCTURE_EXCLUDE_DIRS: string[] = [
  'node_modules',
  '.git',
  'dist',
  'build',
  'out',
  'coverage',
  'target',
  '.next',
  '.cache',
];

function computeEffectiveExcludeDirs(
  searchPath: string,
  excludeDir: string[] | undefined
): string[] {
  const rawExcludeDirs = excludeDir ?? DEFAULT_VIEW_STRUCTURE_EXCLUDE_DIRS;
  const searchPathParts = new Set(searchPath.split('/').filter(Boolean));
  return rawExcludeDirs.filter(dir => !searchPathParts.has(dir));
}

export async function viewStructure(
  query: ViewStructureQuery
): Promise<LocalViewStructureToolResult> {
  try {
    const pathValidation = validateToolPath(
      query,
      TOOL_NAMES.LOCAL_VIEW_STRUCTURE
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as LocalViewStructureToolResult;
    }

    const effectiveShowModified =
      query.detail === 'modified' ||
      query.detail === 'full' ||
      query.sortBy === 'time';

    return await viewStructureNative(
      query,
      pathValidation.sanitizedPath,
      effectiveShowModified
    );
  } catch (error) {
    const toolError = ToolErrors.toolExecutionFailed(
      'LOCAL_VIEW_STRUCTURE',
      error instanceof Error ? error : undefined
    );
    return {
      status: 'error',
      error: toolError.message,
      errorCode: toolError.errorCode,
    };
  }
}

async function viewStructureNative(
  query: ViewStructureQuery,
  basePath: string,
  showModified: boolean = false
): Promise<LocalViewStructureToolResult> {
  const recursiveMode = Boolean(query.maxDepth || query.recursive);
  const maxDepth = recursiveMode
    ? query.maxDepth || (query.recursive ? 5 : 2)
    : 1;
  const nativeNamePatterns = nativeNamePatternsFromQuery(query);
  const hasPostNativeFilter = hasPostNativeFilters(query, nativeNamePatterns);
  const maxEntries =
    recursiveMode && hasPostNativeFilter ? LOCAL_MAX_LIMIT : 10000;
  const excludeDir = computeEffectiveExcludeDirs(
    basePath,
    (query as { excludeDir?: string[] }).excludeDir
  );

  let nativeResult: Awaited<ReturnType<typeof contextUtils.queryFileSystem>>;
  try {
    nativeResult = await contextUtils.queryFileSystem({
      path: basePath,
      recursive: recursiveMode,
      includeRoot: false,
      showHidden: query.hidden ?? false,
      maxDepth,
      names: nativeNamePatterns,
      extensions: query.extensions,
      entryType: nativeEntryTypeFromQuery(query),
      excludeDir,
      limit: maxEntries,
    });
  } catch (error) {
    return createNativeAccessErrorResult(error, query, basePath);
  }

  const entries = nativeResult.entries.map(entry =>
    nativeEntryToDirectoryEntry(entry, showModified, query.detail === 'full')
  );

  let filteredEntries = applyEntryFilters(entries, query);

  const sortBy = query.sortBy ?? 'name';
  filteredEntries = filteredEntries.sort((a, b) => {
    let comparison = 0;
    switch (sortBy) {
      case 'size': {
        const aSize = a.sizeBytes ?? (a.size ? parseFileSize(a.size) : 0);
        const bSize = b.sizeBytes ?? (b.size ? parseFileSize(b.size) : 0);
        comparison = aSize - bSize;
        break;
      }
      case 'time':
        if (showModified && a.modified && b.modified) {
          comparison = a.modified.localeCompare(b.modified);
        } else {
          comparison = a.name.localeCompare(b.name);
        }
        break;
      case 'extension':
        comparison = (a.extension || '').localeCompare(b.extension || '');
        break;
      case 'name':
      default:
        comparison = a.name.localeCompare(b.name);
        break;
    }
    return query.reverse ? -comparison : comparison;
  });

  const availableBeforeLimit = filteredEntries.length;
  if (query.limit) {
    filteredEntries = filteredEntries.slice(0, query.limit);
  }
  const limitTruncated = filteredEntries.length < availableBeforeLimit;
  const scanTruncated = nativeResult.wasCapped;

  const totalEntries = filteredEntries.length;
  const { paginatedEntries, pagination } = paginateEntries(
    filteredEntries,
    query as { itemsPerPage?: number; page?: number }
  );
  const richEntries = query.detail === 'full' || query.detail === 'modified';
  const entryPayload = richEntries
    ? {
        path: basePath,
        entries: paginatedEntries.map(entry => ({
          ...toEntryObject(entry),
          path: entry.path ?? `${basePath.replace(/\/$/, '')}/${entry.name}`,
        })),
      }
    : { path: basePath, ...toGroupedLists(paginatedEntries) };
  const warnings = [
    ...nativeResult.warnings,
    ...buildWalkWarnings({
      skipped: nativeResult.skipped,
      permissionDenied: nativeResult.permissionDenied,
    }),
    ...(nativeResult.wasCapped
      ? [
          `Results capped at ${maxEntries} entries during the walk before sorting — sortBy:"${sortBy}" only orders that partial set, not the true top-N across the whole tree. Add pattern/extensions/entryType/excludeDir or reduce depth to narrow the scope.`,
        ]
      : []),
    ...(pagination.outOfRange
      ? [
          `page:${(query as { page?: number }).page} is out of range (only ${pagination.totalPages} page(s), ${pagination.totalEntries} total entries) — returned page ${pagination.currentPage} instead.`,
        ]
      : []),
  ];
  const isEmpty = totalEntries === 0;
  const summary = summarizeEntries(filteredEntries);

  // Per-result evidence hints (read the first file / descend into the first
  // subdirectory) plus the pagination continuation when there are more pages.
  const rowNext = buildViewStructureNextMap(paginatedEntries) ?? {};
  const requestedLimit = query.limit ?? LOCAL_MAX_LIMIT;
  const canExpandLimit = limitTruncated && requestedLimit < LOCAL_MAX_LIMIT;
  const terminalLimit =
    (pagination.hasMore && pagination.currentPage >= MAX_PAGE_NUMBER) ||
    ((limitTruncated || scanTruncated) && !canExpandLimit);
  const next: Record<string, unknown> = {
    ...rowNext,
    ...(pagination.hasMore && !terminalLimit
      ? {
          nextPage: buildNextPageContinuation(
            TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
            {
              ...query,
              page: pagination.currentPage + 1,
            } as Record<string, unknown>,
            'Continue to the next page of directory entries.'
          ),
        }
      : {}),
    ...(canExpandLimit
      ? {
          expandLimit: buildNextPageContinuation(
            TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
            {
              ...query,
              limit: Math.min(
                LOCAL_MAX_LIMIT,
                Math.max(requestedLimit + 1, requestedLimit * 2)
              ),
              page: 1,
            } as Record<string, unknown>,
            'Re-run with a larger entry limit because directory entries remain.'
          ),
        }
      : {}),
  };

  return attachRawResponseChars(
    {
      ...(isEmpty ? { status: 'empty' as const } : {}),
      ...entryPayload,
      summary,
      ...(pagination.hasMore ||
      pagination.totalPages > 1 ||
      pagination.outOfRange
        ? { pagination }
        : {}),
      ...(Object.keys(next).length > 0 ? { next } : {}),
      ...(terminalLimit ? { terminalLimit: true } : {}),
      ...(limitTruncated || scanTruncated
        ? {
            truncated: true,
            partialReasons: [
              ...(limitTruncated ? ['limit' as const] : []),
              ...(scanTruncated ? ['walkLimit' as const] : []),
            ],
            totalAvailable: Math.max(
              nativeResult.totalDiscovered,
              availableBeforeLimit
            ),
          }
        : {}),
      ...(warnings.length > 0 && { warnings }),
    },
    nativeResult.entries.reduce((sum, entry) => sum + entry.path.length, 0)
  );
}

function hasPostNativeFilters(
  query: ViewStructureQuery,
  nativeNamePatterns: string[] | undefined
): boolean {
  const pattern =
    typeof (query as { pattern?: unknown }).pattern === 'string'
      ? (query as { pattern?: string }).pattern
      : undefined;
  return Boolean(pattern && !nativeNamePatterns);
}

function nativeNamePatternsFromQuery(
  query: ViewStructureQuery
): string[] | undefined {
  const pattern =
    typeof (query as { pattern?: unknown }).pattern === 'string'
      ? (query as { pattern?: string }).pattern
      : undefined;
  if (!pattern) return undefined;

  if (pattern.includes('[')) return undefined;
  return pattern.includes('*') || pattern.includes('?')
    ? [pattern]
    : [`*${pattern}*`];
}

function nativeEntryTypeFromQuery(
  query: ViewStructureQuery
): 'f' | 'd' | undefined {
  return query.entryType;
}

function nativeEntryToDirectoryEntry(
  entry: FileSystemEntry,
  showModified: boolean,
  showDetails: boolean
): DirectoryEntry {
  const type =
    entry.entryType === 'directory'
      ? 'directory'
      : entry.entryType === 'symlink'
        ? 'symlink'
        : 'file';
  const result: DirectoryEntry = {
    name: entry.relativePath || entry.name,
    path: entry.path,
    type,
    ...(entry.size !== undefined
      ? { size: formatFileSize(entry.size), sizeBytes: entry.size }
      : {}),
    ...(entry.extension ? { extension: entry.extension } : {}),
    depth: entry.depth,
  };
  if ((showDetails || showModified) && entry.modifiedMs !== undefined) {
    result.modified = new Date(entry.modifiedMs).toISOString();
  }
  if (showDetails && entry.permissions) {
    result.permissions = octalToSymbolicPermissions(entry.permissions);
  }
  return result;
}

function octalToSymbolicPermissions(octal: string): string {
  const value = Number.parseInt(octal, 8);
  if (!Number.isFinite(value)) return octal;
  const chars = ['---', '--x', '-w-', '-wx', 'r--', 'r-x', 'rw-', 'rwx'];
  return `${chars[(value >> 6) & 7]}${chars[(value >> 3) & 7]}${chars[value & 7]}`;
}

function createNativeAccessErrorResult(
  error: unknown,
  query: ViewStructureQuery,
  basePath: string
): LocalViewStructureToolResult {
  const message = error instanceof Error ? error.message : String(error);
  const isNotFound = /ENOENT|not found|no such file/i.test(message);
  const isPermission = /EACCES|permission denied/i.test(message);
  const isNotDirectory = /ENOTDIR|not a directory/i.test(message);
  const toolError = ToolErrors.pathValidationFailed(
    basePath,
    isNotFound
      ? `Directory not found: ${basePath}`
      : isPermission
        ? `Permission denied: ${basePath}`
        : isNotDirectory
          ? `Not a directory: ${basePath}`
          : `Cannot access path: ${basePath}`
  );
  return createErrorResult(toolError, query, {
    toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  }) as LocalViewStructureToolResult;
}
