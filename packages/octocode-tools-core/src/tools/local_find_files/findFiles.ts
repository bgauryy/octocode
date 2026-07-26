import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { formatFileSize } from '../../utils/file/size.js';
import type { z } from 'zod';
import type { FindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { LocalFindFilesEntry } from '@octocodeai/octocode-core/types';
import type { LocalFindFilesToolResult } from '@octocodeai/octocode-core/extra-types';
import {
  contextUtils,
  type FileSystemEntry,
} from '../../utils/contextUtils.js';

type UpstreamFindFilesQuery = z.infer<typeof FindFilesQuerySchema>;
import type { WithOptionalMeta } from '../../types/execution.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LOCAL_DEFAULT_FILES_PER_PAGE, LOCAL_MAX_LIMIT } from '../../config.js';

import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { buildNextPageContinuation } from '../../scheme/pagination.js';
import { buildWalkWarnings } from '../local_view_structure/structureResponse.js';
import { buildFindFilesNextMap } from './findFilesNext.js';

type FindFilesQuery = WithOptionalMeta<UpstreamFindFilesQuery>;

// No directories are excluded by default: `find` must never silently hide
// real files (node_modules, build/, dist/, out/, target/, …). Hiding them
// broke inspecting installed apps and compiled artifacts. Callers that want to
// trim a search pass `excludeDir` explicitly.
const DEFAULT_FIND_EXCLUDE_DIRS: string[] = [];

function computeEffectiveExcludeDirs(
  searchPath: string,
  excludeDir: string[] | undefined
): string[] {
  const rawExcludeDirs = excludeDir ?? DEFAULT_FIND_EXCLUDE_DIRS;
  const searchPathParts = new Set(searchPath.split('/').filter(Boolean));
  return rawExcludeDirs.filter(dir => !searchPathParts.has(dir));
}

export async function findFiles(
  query: FindFilesQuery
): Promise<LocalFindFilesToolResult> {
  // `detail` collapses the old details/showFileLastModified booleans:
  // "full" = all metadata, "modified" = just add mtime, "basic" = names only.
  const details = query.detail === 'full';
  const showLastModified = query.detail === 'modified' || details;
  const collectModified =
    showLastModified || (query.sortBy || 'modified') === 'modified';

  try {
    const validation = validateToolPath(query, TOOL_NAMES.LOCAL_FIND_FILES);
    if (!validation.isValid) {
      return validation.errorResult as LocalFindFilesToolResult;
    }

    const queryWithSanitizedPath = {
      ...query,
      path: validation.sanitizedPath,
    };

    const queryWithDefaults = {
      ...queryWithSanitizedPath,
      excludeDir: computeEffectiveExcludeDirs(
        queryWithSanitizedPath.path,
        queryWithSanitizedPath.excludeDir
      ),
    };

    // Malformed relative-duration filters are stripped (not just warned about)
    // so the native walk never applies a filter the caller was told was skipped.
    const { warnings: timeFormatWarnings, query: nativeQuery } =
      validateTimeFilterFormats(queryWithDefaults);

    const requestedLimit = query.limit ?? LOCAL_MAX_LIMIT;
    const nativeResult = contextUtils.queryFileSystem({
      path: nativeQuery.path,
      recursive: true,
      includeRoot: true,
      showHidden: true,
      maxDepth: nativeQuery.maxDepth,
      minDepth: nativeQuery.minDepth,
      names: nativeQuery.names,
      pathPattern: nativeQuery.pathPattern,
      regex: nativeQuery.regex,
      entryType: nativeQuery.entryType,
      empty: nativeQuery.empty,
      modifiedWithin: nativeQuery.time?.modifiedWithin,
      modifiedBefore: nativeQuery.time?.modifiedBefore,
      accessedWithin: nativeQuery.time?.accessedWithin,
      sizeGreater: nativeQuery.size?.greater,
      sizeLess: nativeQuery.size?.less,
      permissions: nativeQuery.permissions,
      executable: nativeQuery.access === 'executable',
      readable: nativeQuery.access === 'readable',
      writable: nativeQuery.access === 'writable',
      excludeDir: nativeQuery.excludeDir,
      limit: LOCAL_MAX_LIMIT,
    });

    const discoveredFileCount = nativeResult.totalDiscovered;
    const wasFileCapped = nativeResult.wasCapped;
    const files = nativeResult.entries.map(entry =>
      nativeEntryToFindFile(entry, collectModified)
    );
    const sortBy = query.sortBy || 'modified';
    sortLocalFindFilesEntrys(files, sortBy, collectModified);

    const limitedFiles = files.slice(0, requestedLimit);
    const filesForOutput = formatForOutput(
      limitedFiles,
      details,
      showLastModified
    );
    const totalFiles = filesForOutput.length;

    const filesPerPage =
      (query as { itemsPerPage?: number }).itemsPerPage ||
      LOCAL_DEFAULT_FILES_PER_PAGE;
    const currentPage = (query as { page?: number }).page || 1;
    const totalPages = Math.max(1, Math.ceil(totalFiles / filesPerPage));
    const startIdx = (currentPage - 1) * filesPerPage;
    const endIdx = Math.min(startIdx + filesPerPage, totalFiles);
    const paginatedFiles = filesForOutput.slice(startIdx, endIdx);

    const finalFiles = paginatedFiles;

    const nativeWarnings = [
      ...nativeResult.warnings,
      ...buildWalkWarnings(nativeResult),
    ];
    const allWarnings = [...timeFormatWarnings, ...nativeWarnings];

    const hasMore = currentPage < totalPages;
    // Per-result evidence hints (read the first file / orient into the first
    // dir) plus the pagination continuation when there are more pages.
    const rowNext = buildFindFilesNextMap(finalFiles) ?? {};
    const next: Record<string, unknown> = {
      ...rowNext,
      ...(hasMore
        ? {
            nextPage: buildNextPageContinuation(
              TOOL_NAMES.LOCAL_FIND_FILES,
              {
                ...queryWithSanitizedPath,
                page: currentPage + 1,
              } as Record<string, unknown>,
              'Continue to the next page of matched files.'
            ),
          }
        : {}),
    };
    const fullResult: LocalFindFilesToolResult = {
      ...(totalFiles === 0 ? { status: 'empty' as const } : {}),
      path: queryWithSanitizedPath.path,
      files: finalFiles,
      pagination: {
        currentPage,
        totalPages,
        filesPerPage,
        totalFiles,
        hasMore,
        ...(hasMore ? { nextPage: currentPage + 1 } : {}),
        ...(wasFileCapped || discoveredFileCount > totalFiles
          ? { totalFilesFound: discoveredFileCount }
          : {}),
      },
      ...(Object.keys(next).length > 0 ? { next } : {}),
      ...(allWarnings.length > 0 && { warnings: allWarnings }),
    };

    return attachRawResponseChars(
      fullResult,
      nativeResult.entries.reduce((sum, entry) => sum + entry.path.length, 0)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
    }) as LocalFindFilesToolResult;
  }
}

function nativeEntryToFindFile(
  entry: FileSystemEntry,
  showLastModified: boolean
): LocalFindFilesEntry {
  const file: LocalFindFilesEntry = {
    path: entry.path,
    type:
      entry.entryType === 'directory'
        ? 'directory'
        : entry.entryType === 'symlink'
          ? 'symlink'
          : 'file',
    ...(entry.size !== undefined ? { size: entry.size } : {}),
    ...(entry.permissions ? { permissions: entry.permissions } : {}),
  };
  if (showLastModified && entry.modifiedMs !== undefined) {
    file.modified = new Date(entry.modifiedMs).toISOString();
  }
  return file;
}

function sortLocalFindFilesEntrys(
  files: LocalFindFilesEntry[],
  sortBy: string,
  showLastModified: boolean
): void {
  files.sort((a, b) => {
    switch (sortBy) {
      case 'size':
        return (b.size ?? 0) - (a.size ?? 0);
      case 'name':
        return (a.path.split('/').pop() || '').localeCompare(
          b.path.split('/').pop() || ''
        );
      case 'path':
        return a.path.localeCompare(b.path);
      case 'modified':
      default:
        if (showLastModified && a.modified && b.modified) {
          return (
            new Date(b.modified).getTime() - new Date(a.modified).getTime()
          );
        }
        return a.path.localeCompare(b.path);
    }
  });
}

function formatForOutput(
  files: LocalFindFilesEntry[],
  details: boolean,
  _showLastModified: boolean
): LocalFindFilesEntry[] {
  return files.map(f => {
    const result: LocalFindFilesEntry = { path: f.path, type: f.type };
    if (f.size !== undefined && f.type !== 'directory') {
      // One size per mode: human label by default, numeric in details mode
      // (OQL files-lane sorting needs the number, never both).
      if (details) result.size = f.size;
      else result.sizeFormatted = formatFileSize(f.size);
    }
    if (details && f.permissions) result.permissions = f.permissions;
    if (f.modified) result.modified = f.modified;
    return result;
  });
}

const VALID_TIME_STRING_RE = /^\d+[hdwm]$/;

type TimeFilterKey = 'modifiedBefore' | 'modifiedWithin' | 'accessedWithin';

// Validate the relative-duration time filters and strip any that are malformed,
// so the returned query only carries filters the native walk will actually
// honour. Callers surface `warnings` and pass `query` to queryFileSystem.
function validateTimeFilterFormats<T extends FindFilesQuery>(
  query: T
): {
  warnings: string[];
  query: T;
} {
  const warnings: string[] = [];
  const time = query.time;
  if (!time) return { warnings, query };
  const sanitizedTime = { ...time };
  const fields: Array<{ key: TimeFilterKey; value: string | undefined }> = [
    { key: 'modifiedBefore', value: time.modifiedBefore },
    { key: 'modifiedWithin', value: time.modifiedWithin },
    { key: 'accessedWithin', value: time.accessedWithin },
  ];
  for (const { key, value } of fields) {
    if (value && !VALID_TIME_STRING_RE.test(value)) {
      warnings.push(
        `time.${key}="${value}" has an unsupported format — filter was skipped. Use a relative duration like "7d", "2h", "1w", or "3m".`
      );
      delete sanitizedTime[key];
    }
  }
  return { warnings, query: { ...query, time: sanitizedTime } };
}
