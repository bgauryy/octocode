import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
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
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import {
  contextUtils,
  type FileSystemEntry,
} from '../../utils/contextUtils.js';
import { buildNextPageContinuation } from '../../scheme/pagination.js';

type ViewStructureQuery = WithOptionalMeta<LocalViewStructureQuery>;

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
      query.showFileLastModified ??
      (query.sortBy === 'time' || query.details === true);

    return viewStructureNative(
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

function viewStructureNative(
  query: ViewStructureQuery,
  basePath: string,
  showModified: boolean = false
): LocalViewStructureToolResult {
  const recursiveMode = Boolean(query.maxDepth || query.recursive);
  const maxDepth = recursiveMode
    ? query.maxDepth || (query.recursive ? 5 : 2)
    : 1;
  const nativeNamePatterns = nativeNamePatternsFromQuery(query);
  const maxEntries =
    recursiveMode &&
    query.limit &&
    !hasPostNativeFilters(query, nativeNamePatterns)
      ? query.limit * 2
      : 10000;

  let nativeResult: ReturnType<typeof contextUtils.queryFileSystem>;
  try {
    nativeResult = contextUtils.queryFileSystem({
      path: basePath,
      recursive: recursiveMode,
      includeRoot: false,
      showHidden: query.hidden ?? false,
      maxDepth,
      names: nativeNamePatterns,
      extensions: query.extensions,
      entryType: nativeEntryTypeFromQuery(query),
      limit: maxEntries,
    });
  } catch (error) {
    return createNativeAccessErrorResult(error, query, basePath);
  }

  const entries = nativeResult.entries.map(entry =>
    nativeEntryToDirectoryEntry(entry, showModified, query.details ?? false)
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

  if (query.limit) {
    filteredEntries = filteredEntries.slice(0, query.limit);
  }

  const totalEntries = filteredEntries.length;
  const { paginatedEntries, pagination } = paginateEntries(
    filteredEntries,
    query as { itemsPerPage?: number; page?: number }
  );
  const richEntries =
    query.details === true || query.showFileLastModified === true;
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
          `Results capped at ${maxEntries} entries — add a pattern/extensions filter or reduce depth to narrow the scope.`,
        ]
      : []),
  ];
  const isEmpty = totalEntries === 0;
  const summary = summarizeEntries(filteredEntries);

  return attachRawResponseChars(
    finalizeViewStructureResult(
      {
        ...(isEmpty ? { status: 'empty' as const } : {}),
        ...entryPayload,
        summary,
        ...(pagination.hasMore || pagination.totalPages > 1
          ? { pagination }
          : {}),
        ...(pagination.hasMore
          ? {
              next: {
                nextPage: buildNextPageContinuation(
                  TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
                  {
                    ...query,
                    page: pagination.currentPage + 1,
                  } as Record<string, unknown>,
                  'Continue to the next page of directory entries.'
                ),
              },
            }
          : {}),
        ...(warnings.length > 0 && { warnings }),
      },
      query
    ),
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
  if (query.filesOnly && !query.directoriesOnly) return 'f';
  if (query.directoriesOnly && !query.filesOnly) return 'd';
  return undefined;
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

export function finalizeViewStructureResult(
  result: LocalViewStructureToolResult,
  _query: ViewStructureQuery
): LocalViewStructureToolResult {
  return result;
}
