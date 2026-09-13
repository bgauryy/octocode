import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { computeEffectiveExcludeDirs } from './defaults.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../../utils/file/toolHelpers.js';
import { formatFileSize } from '../../../utils/file/size.js';
import type { AstSearchQuery } from '@octocodeai/octocode-core/schema';
import type { AstFilesEntry } from '@octocodeai/octocode-core/types';
import type { AstFilesToolResult } from '@octocodeai/octocode-core/extra-types';
import { contextUtils } from '../../../utils/contextUtils.js';
import type { FileSystemEntry } from '@octocodeai/octocode-engine';

import { AST_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import {
  LOCAL_DEFAULT_FILES_PER_PAGE,
  LOCAL_MAX_LIMIT,
  MAX_PAGE_NUMBER,
} from '@octocodeai/octocode-core/schema';

import { attachRawResponseChars } from '../../../utils/response/charSavings.js';
import { buildNextPageContinuation } from '../../../scheme/pagination.js';
import { buildWalkWarnings } from './response.js';
import { buildFilesNextMap } from './filesNext.js';

type FindFilesQuery = Extract<AstSearchQuery, { operation: 'files' }>;

export async function findFiles(
  query: FindFilesQuery
): Promise<AstFilesToolResult> {
  const details = query.detail === 'full';
  const showLastModified = query.detail === 'modified' || details;
  const collectModified =
    showLastModified || (query.sort || 'modified') === 'modified';

  try {
    const validation = validateToolPath(query, AST_SEARCH_TOOL_NAME);
    if (!validation.isValid) {
      return validation.errorResult as AstFilesToolResult;
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
    const baseNativeQueryOptions = {
      path: nativeQuery.path,
      recursive: true,
      includeRoot: true,
      showHidden: true,
      maxDepth: nativeQuery.maxDepth,
      minDepth: nativeQuery.minDepth,
      names: nativeQuery.names,
      extensions: nativeQuery.extensions,
      regex: nativeQuery.pathRegex,
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
    };
    const nativeResult = await contextUtils.queryFileSystem({
      ...baseNativeQueryOptions,
      pathPattern: nativeQuery.pathPattern,
    });

    const discoveredFileCount = nativeResult.totalDiscovered;
    const wasFileCapped = nativeResult.wasCapped;
    const files = nativeResult.entries.map(entry =>
      nativeEntryToFindFile(entry, collectModified)
    );
    const sort = query.sort || 'modified';
    // Line counts are needed when sorting by lines or showing detail:full.
    // Cap at 2000 files to keep latency bounded.
    if ((sort === 'lines' || details) && files.length <= 2000) {
      for (const file of files) {
        if (file.type !== 'directory') {
          file.lineCount = countFileLines(
            resolve(queryWithSanitizedPath.path, file.path)
          );
        }
      }
    }
    sortFileEntries(files, sort, collectModified);

    const limitedFiles = files.slice(0, requestedLimit);
    const limitTruncated = limitedFiles.length < files.length;
    const scanTruncated = wasFileCapped;
    const filesForOutput = formatForOutput(limitedFiles, details);
    const totalFiles = filesForOutput.length;

    const filesPerPage = query.pageSize || LOCAL_DEFAULT_FILES_PER_PAGE;
    const currentPage = query.page || 1;
    const totalPages = Math.max(1, Math.ceil(totalFiles / filesPerPage));
    const startIdx = (currentPage - 1) * filesPerPage;
    const endIdx = Math.min(startIdx + filesPerPage, totalFiles);
    const paginatedFiles = filesForOutput.slice(startIdx, endIdx);
    // A `page` beyond `totalPages` makes `startIdx` exceed `totalFiles`, so
    // `.slice()` silently returns [] — indistinguishable from "you're on a
    // valid last page with no more results" unless flagged explicitly.
    const isPageOutOfRange = totalFiles > 0 && startIdx >= totalFiles;

    const finalFiles = paginatedFiles;

    const nativeWarnings = [
      ...nativeResult.warnings,
      ...buildWalkWarnings(nativeResult),
      // The native walk stops at LOCAL_MAX_LIMIT entries *during* traversal,
      // before this file sorts — so a capped result's sort only orders
      // whatever arbitrary subset the walk reached first, not the true
      // top-N by `sort` across the whole tree. Narrow with excludeDir,
      // names, or a deeper path to get a sort that covers everything.
      ...(wasFileCapped
        ? [
            `results capped at ${LOCAL_MAX_LIMIT} during the walk before sorting — sort:"${sort}" only orders that partial set, not the true top-N across the whole tree`,
          ]
        : []),
      ...(isPageOutOfRange
        ? [
            `page:${currentPage} is out of range (only ${totalPages} page(s), ${totalFiles} total file(s)) — returned 0 files. Use page:1..${totalPages}.`,
          ]
        : []),
    ];
    const allWarnings = [...timeFormatWarnings, ...nativeWarnings];

    const hasMore = currentPage < totalPages;
    const canExpandLimit = limitTruncated && requestedLimit < LOCAL_MAX_LIMIT;
    const terminalLimit =
      (hasMore && currentPage >= MAX_PAGE_NUMBER) ||
      ((limitTruncated || scanTruncated) && !canExpandLimit);
    // Per-result evidence hints (read the first file / orient into the first
    // dir) plus the pagination continuation when there are more pages.
    const rowNext = buildFilesNextMap(finalFiles) ?? {};
    const next: Record<string, unknown> = {
      ...rowNext,
      ...(hasMore && !terminalLimit
        ? {
            nextPage: buildNextPageContinuation(
              AST_SEARCH_TOOL_NAME,
              {
                ...queryWithSanitizedPath,
                page: currentPage + 1,
              } as Record<string, unknown>,
              'Continue to the next page of matched files.'
            ),
          }
        : {}),
      ...(canExpandLimit
        ? {
            expandLimit: buildNextPageContinuation(
              AST_SEARCH_TOOL_NAME,
              {
                ...queryWithSanitizedPath,
                limit: Math.min(
                  LOCAL_MAX_LIMIT,
                  Math.max(requestedLimit + 1, requestedLimit * 2)
                ),
                page: 1,
              } as Record<string, unknown>,
              'Re-run with a larger file limit because matched files remain.'
            ),
          }
        : {}),
    };
    const fullResult = {
      ...(totalFiles === 0 ? { status: 'empty' as const } : {}),
      path: queryWithSanitizedPath.path,
      files: finalFiles,
      pagination: {
        currentPage,
        totalPages,
        filesPerPage,
        totalFiles,
        hasMore,
        ...(hasMore && !terminalLimit ? { nextPage: currentPage + 1 } : {}),
        ...(wasFileCapped || discoveredFileCount > totalFiles
          ? { totalFilesFound: discoveredFileCount }
          : {}),
        ...(isPageOutOfRange ? { outOfRange: true } : {}),
      },
      ...(terminalLimit ? { terminalLimit: true } : {}),
      ...(limitTruncated || scanTruncated
        ? {
            truncated: true,
            partialReasons: [
              ...(limitTruncated ? ['limit' as const] : []),
              ...(scanTruncated ? ['walkLimit' as const] : []),
            ],
            totalAvailable: Math.max(discoveredFileCount, files.length),
          }
        : {}),
      ...(Object.keys(next).length > 0 ? { next } : {}),
      ...(allWarnings.length > 0 && { warnings: allWarnings }),
    } as AstFilesToolResult & { terminalLimit?: boolean };

    return attachRawResponseChars(
      fullResult,
      nativeResult.entries.reduce((sum, entry) => sum + entry.path.length, 0)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: AST_SEARCH_TOOL_NAME,
    }) as AstFilesToolResult;
  }
}

function nativeEntryToFindFile(
  entry: FileSystemEntry,
  showLastModified: boolean
): AstFilesEntry {
  const file: AstFilesEntry = {
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

/** Count newlines in a file; returns undefined on any read error. */
function countFileLines(absPath: string): number | undefined {
  try {
    const buf = readFileSync(absPath);
    let count = 1;
    for (let i = 0; i < buf.length; i++) {
      if (buf[i] === 10) count++; // 0x0A = '\n'
    }
    return count;
  } catch {
    return undefined;
  }
}

function sortFileEntries(
  files: AstFilesEntry[],
  sort: string,
  showLastModified: boolean
): void {
  files.sort((a, b) => {
    switch (sort) {
      case 'lines':
        return (b.lineCount ?? 0) - (a.lineCount ?? 0);
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
  files: AstFilesEntry[],
  details: boolean
): AstFilesEntry[] {
  return files.map(f => {
    const result: AstFilesEntry = { path: f.path, type: f.type };
    if (f.size !== undefined && f.type !== 'directory') {
      // One size per mode: human label by default, numeric in details mode
      // (sort:"size" needs the number, never both).
      if (details) result.size = f.size;
      else result.sizeFormatted = formatFileSize(f.size);
    }
    if (details && f.lineCount !== undefined && f.type !== 'directory')
      result.lineCount = f.lineCount;
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
