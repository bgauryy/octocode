import { getHints } from '../../hints/index.js';
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

const DEFAULT_FIND_EXCLUDE_DIRS = [
  'node_modules',
  'dist',
  '.git',
  'coverage',
  'build',
  '.next',
  '.nuxt',
  '.svelte-kit',
  '.turbo',
  '.cache',
  '.parcel-cache',
  'out',
  'target',
  '.octocode',
  '.cursor',
  '.vscode',
  '.idea',
  '.claude',
  '.context',
];

function computeEffectiveExcludeDirs(
  searchPath: string,
  excludeDir: string[] | undefined
): string[] {
  const rawExcludeDirs = excludeDir ?? DEFAULT_FIND_EXCLUDE_DIRS;
  const searchPathParts = new Set(searchPath.split('/').filter(Boolean));
  return rawExcludeDirs.filter(dir => !searchPathParts.has(dir));
}

function buildFindFilesHints(ctx: {
  query: FindFilesQuery;
  currentPage: number;
  totalPages: number;
  shownCount: number;
  totalFiles: number;
  wasFileCapped: boolean;
  maxFiles: number;
  discoveredFileCount: number;
  hasConfigFiles: boolean;
  extraHints?: string[];
}): string[] {
  const {
    query,
    currentPage,
    totalPages,
    shownCount,
    totalFiles,
    wasFileCapped,
    maxFiles,
    discoveredFileCount,
    hasConfigFiles,
    extraHints = [],
  } = ctx;

  const q = query as Record<string, unknown>;
  const activeFilters: string[] = [];
  if (Array.isArray(q.names) && q.names.length > 0) {
    activeFilters.push(`names: ${(q.names as string[]).join(', ')}`);
  }
  if (q.entryType)
    activeFilters.push(
      `entryType: ${q.entryType === 'f' ? 'files' : q.entryType === 'd' ? 'directories' : String(q.entryType)}`
    );
  const timeFilterNote = (value: unknown): string =>
    typeof value === 'string' && !VALID_TIME_STRING_RE.test(value)
      ? ' (skipped: invalid format)'
      : '';
  if (q.modifiedBefore)
    activeFilters.push(
      `modified before: ${q.modifiedBefore}${timeFilterNote(q.modifiedBefore)}`
    );
  if (q.modifiedWithin)
    activeFilters.push(
      `modified within: ${q.modifiedWithin}${timeFilterNote(q.modifiedWithin)}`
    );
  if (q.sizeGreater) activeFilters.push(`size > ${q.sizeGreater}`);
  if (q.sizeLess) activeFilters.push(`size < ${q.sizeLess}`);
  if (Array.isArray(q.excludeDir) && q.excludeDir.length > 0) {
    activeFilters.push(`excluding: ${(q.excludeDir as string[]).join(', ')}`);
  }

  return [
    ...extraHints,
    ...(activeFilters.length > 0
      ? [`Active filters — ${activeFilters.join(' | ')}`]
      : []),
    ...(currentPage < totalPages
      ? [
          `Page ${currentPage}/${totalPages} (${shownCount} of ${totalFiles}). Next: page=${currentPage + 1}`,
        ]
      : []),
    ...(totalPages > 0 && currentPage > totalPages
      ? [
          `Requested page ${currentPage} is outside available range (1-${totalPages}). Use page=${totalPages} for the last page.`,
        ]
      : []),
    ...(wasFileCapped
      ? [
          `Results capped at ${maxFiles} of ${discoveredFileCount} discovered. All ${maxFiles} are reachable via page; to see the rest, narrow with names/entryType/time filters. Note: sorting applies only within the capped set — limit is a pre-sort discovery cap.`,
        ]
      : []),
    ...(totalFiles === 0
      ? getHints(TOOL_NAMES.LOCAL_FIND_FILES, 'empty', {
          fileCount: totalFiles,
          hasConfigFiles,
          path: query.path,
          names: query.names,
          modifiedWithin: query.modifiedWithin,
          sizeGreater: query.sizeGreater,
          sizeLess: query.sizeLess,
        } as Record<string, unknown>)
      : [
          q.entryType === 'f'
            ? `Found ${totalFiles} file${totalFiles === 1 ? '' : 's'}. Use localSearchCode to search or localGetFileContent to read.`
            : q.entryType === 'd'
              ? `Found ${totalFiles} director${totalFiles === 1 ? 'y' : 'ies'}. Use localViewStructure to browse or localSearchCode to search.`
              : `Found ${totalFiles} entr${totalFiles === 1 ? 'y' : 'ies'} — pass entryType="f" for files, entryType="d" for directories. Use localSearchCode or localGetFileContent.`,
        ]),
  ];
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

    const maxFiles = query.limit ?? LOCAL_MAX_LIMIT;
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
      limit: maxFiles,
    });

    const discoveredFileCount = nativeResult.totalDiscovered;
    const wasFileCapped = nativeResult.wasCapped;
    const files = nativeResult.entries.map(entry =>
      nativeEntryToFindFile(entry, collectModified)
    );
    const sortBy = query.sortBy || 'modified';
    sortLocalFindFilesEntrys(files, sortBy, collectModified);
    const sortHints: string[] = [];

    const filesForOutput = formatForOutput(files, details, showLastModified);
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
    const configFilePatterns =
      /\.(config|rc|env|json|ya?ml|toml|ini)$|^(\..*rc|config\.|\.env)/i;
    const hasConfigFiles = finalFiles.some(f =>
      configFilePatterns.test(f.path.split('/').pop() || '')
    );

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
        ...(wasFileCapped ? { totalFilesFound: discoveredFileCount } : {}),
      },
      ...(allWarnings.length > 0 && { warnings: allWarnings }),
      hints: buildFindFilesHints({
        query,
        currentPage,
        totalPages,
        shownCount: finalFiles.length,
        totalFiles,
        wasFileCapped,
        maxFiles,
        discoveredFileCount,
        hasConfigFiles,
        extraHints: sortHints,
      }),
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
        | string
        | undefined,
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
