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
  const details = query.details ?? false;
  const showLastModified = query.showFileLastModified ?? false;
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

    const timeFormatWarnings = validateTimeFilterFormats(queryWithDefaults);

    const requestedLimit = query.limit ?? LOCAL_MAX_LIMIT;
    const nativeResult = contextUtils.queryFileSystem({
      path: queryWithDefaults.path,
      recursive: true,
      includeRoot: true,
      showHidden: true,
      maxDepth: queryWithDefaults.maxDepth,
      minDepth: queryWithDefaults.minDepth,
      names: queryWithDefaults.names,
      pathPattern: queryWithDefaults.pathPattern,
      regex: queryWithDefaults.regex,
      entryType: queryWithDefaults.entryType,
      empty: queryWithDefaults.empty,
      modifiedWithin: queryWithDefaults.modifiedWithin,
      modifiedBefore: queryWithDefaults.modifiedBefore,
      accessedWithin: queryWithDefaults.accessedWithin,
      sizeGreater: queryWithDefaults.sizeGreater,
      sizeLess: queryWithDefaults.sizeLess,
      permissions: queryWithDefaults.permissions,
      executable: queryWithDefaults.executable,
      readable: queryWithDefaults.readable,
      writable: queryWithDefaults.writable,
      excludeDir: queryWithDefaults.excludeDir,
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
      ...(nativeResult.skipped > 0
        ? [
            `${nativeResult.skipped} entr${nativeResult.skipped === 1 ? 'y' : 'ies'} skipped during filesystem traversal`,
          ]
        : []),
    ];
    const allWarnings = [...timeFormatWarnings, ...nativeWarnings];

    const fullResult: LocalFindFilesToolResult = {
      ...(totalFiles === 0 ? { status: 'empty' as const } : {}),
      path: queryWithSanitizedPath.path,
      files: finalFiles,
      pagination: {
        currentPage,
        totalPages,
        filesPerPage,
        totalFiles,
        hasMore: currentPage < totalPages,
        ...(currentPage < totalPages ? { nextPage: currentPage + 1 } : {}),
        ...(wasFileCapped || discoveredFileCount > totalFiles
          ? { totalFilesFound: discoveredFileCount }
          : {}),
      },
      ...(allWarnings.length > 0 && { warnings: allWarnings }),
    };

    return attachRawResponseChars(
      finalizeFindFilesResult(fullResult, query, { totalFiles }),
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

export function finalizeFindFilesResult(
  result: LocalFindFilesToolResult,
  _query: FindFilesQuery,
  _totals: { totalFiles: number }
): LocalFindFilesToolResult {
  return result;
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
      result.sizeFormatted = formatFileSize(f.size);
      // Numeric size rides along in details mode so downstream consumers
      // (e.g. OQL files-lane sorting) can order without parsing the label.
      if (details) result.size = f.size;
    }
    if (details && f.permissions) result.permissions = f.permissions;
    if (f.modified) result.modified = f.modified;
    return result;
  });
}

const VALID_TIME_STRING_RE = /^\d+[hdwm]$/;

function validateTimeFilterFormats(query: FindFilesQuery): string[] {
  const warnings: string[] = [];
  const fields: Array<{ key: string; value: string | undefined }> = [
    { key: 'modifiedBefore', value: query.modifiedBefore },
    { key: 'modifiedWithin', value: query.modifiedWithin },
    {
      key: 'accessedWithin',
      value: (query as Record<string, unknown>).accessedWithin as
        string | undefined,
    },
  ];
  for (const { key, value } of fields) {
    if (value && !VALID_TIME_STRING_RE.test(value)) {
      warnings.push(
        `${key}="${value}" has an unsupported format — filter was skipped. Use a relative duration like "7d", "2h", "1w", or "3m".`
      );
    }
  }
  return warnings;
}
