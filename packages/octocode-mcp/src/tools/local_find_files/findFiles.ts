import { FindCommandBuilder } from '../../commands/FindCommandBuilder.js';
import { safeExec } from '../../utils/exec/safe.js';
import {
  checkCommandAvailability,
  getMissingCommandError,
} from '../../utils/exec/commandAvailability.js';
import { getHints } from '../../hints/index.js';
import { generatePaginationHints } from '../../utils/pagination/hints.js';
import {
  serializeForPagination,
  createPaginationInfo,
} from '../../utils/pagination/core.js';
import type { PaginationMetadata } from '../../utils/pagination/types.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { formatFileSize } from '../../utils/file/size.js';
import type { z } from 'zod/v4';
import type { FindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { LocalFindFilesEntry } from '@octocodeai/octocode-core/types';
import type { LocalFindFilesToolResult } from '@octocodeai/octocode-core/extra-types';

type UpstreamFindFilesQuery = z.infer<typeof FindFilesQuerySchema>;
import type { WithOptionalMeta } from '../../types/execution.js';
import fs from 'fs';
import { ToolErrors } from '../../errors/errorFactories.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import { LOCAL_OVERLAY_MAX_LIMIT } from '../../scheme/localSchemaOverlay.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';

/** Advisory hints localFindFiles emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryFindFilesHint = makeAdvisoryPredicate([
  'excludedir',
  'raw name globs',
  'metadata only',
  'noisy dir',
]);
import { attachRawResponseChars } from '../../utils/response/charSavings.js';

type FindFilesQuery = WithVerbosity<WithOptionalMeta<UpstreamFindFilesQuery>>;

// Directories pruned by default so a find never walks build output / VCS
// metadata. Overridable per-query via `excludeDir`.
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

/**
 * Resolve the exclude-dir prune list for a search.
 *
 * Drops any excludeDir entry that appears as a component of the search path
 * itself — otherwise a search inside e.g. `.context/` would have every result
 * pruned by the `.context` glob prune because each result path contains it.
 */
function computeEffectiveExcludeDirs(
  searchPath: string,
  excludeDir: string[] | undefined
): string[] {
  const rawExcludeDirs = excludeDir ?? DEFAULT_FIND_EXCLUDE_DIRS;
  const searchPathParts = new Set(searchPath.split('/').filter(Boolean));
  return rawExcludeDirs.filter(dir => !searchPathParts.has(dir));
}

/**
 * Backfill size / permissions / mtime for entries the fast path left partial,
 * via a per-file lstat. Failures leave that file's metadata partial (best
 * effort — one unreadable file must not fail the whole listing).
 */
async function enrichFileDetails(
  files: LocalFindFilesEntry[],
  showLastModified: boolean
): Promise<void> {
  await Promise.all(
    files.map(async file => {
      if (
        file.size === undefined ||
        !file.permissions ||
        (showLastModified && !file.modified)
      ) {
        try {
          const stats = await fs.promises.lstat(file.path);
          if (file.size === undefined) file.size = stats.size;
          if (!file.permissions) {
            file.permissions = stats.mode.toString(8).slice(-3);
          }
          if (showLastModified && !file.modified) {
            file.modified = stats.mtime.toISOString();
          }
        } catch {
          // lstat failed for one file; leave metadata partial.
        }
      }
    })
  );
}

/** Assemble the response hints (pagination, cap notice, empty guidance). */
function buildFindFilesHints(ctx: {
  query: FindFilesQuery;
  filePageNumber: number;
  totalPages: number;
  shownCount: number;
  totalFiles: number;
  wasFileCapped: boolean;
  maxFiles: number;
  discoveredFileCount: number;
  hasConfigFiles: boolean;
  extraHints?: string[];
  paginationMetadata:
    | Parameters<typeof generatePaginationHints>[0]
    | null
    | undefined;
}): string[] {
  const {
    query,
    filePageNumber,
    totalPages,
    shownCount,
    totalFiles,
    wasFileCapped,
    maxFiles,
    discoveredFileCount,
    hasConfigFiles,
    extraHints = [],
    paginationMetadata,
  } = ctx;

  // Build active-filter summary so agents know exactly which constraints
  // were applied — silent filters are the #1 source of "why no results" confusion.
  const q = query as Record<string, unknown>;
  const activeFilters: string[] = [];
  const namePattern =
    (q.name as string | undefined) ?? (q.iname as string | undefined);
  if (namePattern) {
    const caseNote = q.iname ? ' (case-insensitive)' : '';
    activeFilters.push(`name: ${namePattern}${caseNote}`);
  }
  if (q.type)
    activeFilters.push(
      `type: ${q.type === 'f' ? 'files' : q.type === 'd' ? 'directories' : String(q.type)}`
    );
  if (q.modifiedAfter) activeFilters.push(`modified after: ${q.modifiedAfter}`);
  if (q.modifiedBefore)
    activeFilters.push(`modified before: ${q.modifiedBefore}`);
  if (q.modifiedWithin)
    activeFilters.push(`modified within: ${q.modifiedWithin}`);
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
    ...(filePageNumber < totalPages
      ? [
          `Page ${filePageNumber}/${totalPages} (showing ${shownCount} of ${totalFiles}). Next: page=${filePageNumber + 1}`,
        ]
      : []),
    // Overshoot: a page past the last one returns empty — signal it explicitly
    // rather than letting an empty result look like "no matches".
    ...(totalPages > 0 && filePageNumber > totalPages
      ? [
          `Requested page ${filePageNumber} is outside available range (1-${totalPages}). Use page=${totalPages} for the last page.`,
        ]
      : []),
    ...(wasFileCapped
      ? [
          `Results capped at ${maxFiles} of ${discoveredFileCount} discovered. All ${maxFiles} are reachable via page; to see the rest, narrow with name/type/time filters.`,
        ]
      : []),
    ...(totalFiles === 0
      ? getHints(TOOL_NAMES.LOCAL_FIND_FILES, 'empty', {
          fileCount: totalFiles,
          hasConfigFiles,
          path: query.path,
          name: query.name ?? query.iname,
          modifiedWithin: query.modifiedWithin,
          sizeGreater: query.sizeGreater,
          sizeLess: query.sizeLess,
        } as Record<string, unknown>)
      : []),
    ...(paginationMetadata
      ? generatePaginationHints(paginationMetadata, {
          toolName: TOOL_NAMES.LOCAL_FIND_FILES,
        })
      : []),
  ];
}

export async function findFiles(
  query: FindFilesQuery
): Promise<LocalFindFilesToolResult> {
  const details = query.details ?? true;
  const showLastModified = query.showFileLastModified ?? true;

  try {
    const findAvailability = await checkCommandAvailability('find');
    if (!findAvailability.available) {
      const toolError = ToolErrors.commandNotAvailable(
        'find',
        getMissingCommandError('find')
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_FIND_FILES,
      }) as LocalFindFilesToolResult;
    }

    const validation = validateToolPath(query, TOOL_NAMES.LOCAL_FIND_FILES);
    if (!validation.isValid) {
      return validation.errorResult as LocalFindFilesToolResult;
    }

    const queryWithSanitizedPath = {
      ...query,
      path: validation.sanitizedPath!,
    };

    const queryWithDefaults = {
      ...queryWithSanitizedPath,
      excludeDir: computeEffectiveExcludeDirs(
        queryWithSanitizedPath.path,
        queryWithSanitizedPath.excludeDir
      ),
    };

    const timeFormatWarnings = validateTimeFilterFormats(queryWithDefaults);

    const builder = new FindCommandBuilder();
    const { command, args } = builder.fromQuery(queryWithDefaults).build();

    const result = await safeExec(command, args);

    if (!result.success) {
      const stderrMsg = result.stderr?.trim();
      const userMessage =
        stderrMsg?.replace(/^find:\s*/i, '').trim() ||
        'File search operation failed';
      const toolError = ToolErrors.commandExecutionFailed(
        'find',
        new Error(userMessage),
        userMessage
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_FIND_FILES,
        extra: { stderr: userMessage },
        rawResponse: result.stdout.length + result.stderr.length,
      }) as LocalFindFilesToolResult;
    }

    let filePaths = result.stdout
      .split('\0')
      .filter(line => line.trim())
      .map(line => line.trim());

    // `limit` is an EXPLICIT user cap. When omitted, the discovery cap matches
    // the documented `limit` maximum (10000, the schema bound) so EVERY
    // discovered file is reachable by paging (page/itemsPerPage) —
    // not silently truncated at an implicit 1000 that pagination can't escape.
    // The cap remains a perf guard (it bounds the stat fan-out); when it bites,
    // the hint below tells the agent to narrow filters. Defaulting to the
    // schema's `limit` max keeps the cap and the documented bound in lock-step.
    const maxFiles = query.limit ?? LOCAL_OVERLAY_MAX_LIMIT;
    const discoveredFileCount = filePaths.length;
    const wasFileCapped = discoveredFileCount > maxFiles;
    filePaths = filePaths.slice(0, maxFiles);

    const files: LocalFindFilesEntry[] = await getFileDetails(
      filePaths,
      showLastModified
    );

    if (details) {
      await enrichFileDetails(files, showLastModified);
    }

    const sortBy = query.sortBy || 'modified';
    sortLocalFindFilesEntrys(files, sortBy, showLastModified);
    const sortHints =
      query.sortBy === 'modified' && !showLastModified
        ? [
            'sortBy="modified" ignored: showFileLastModified=false; sorted by path instead.',
          ]
        : [];

    const filesForOutput = formatForOutput(files, details, showLastModified);
    const totalFiles = filesForOutput.length;

    const filesPerPage =
      (query as { itemsPerPage?: number }).itemsPerPage || 20;
    const filePageNumber = (query as { page?: number }).page || 1;
    const totalPages = Math.ceil(totalFiles / filesPerPage);
    const startIdx = (filePageNumber - 1) * filesPerPage;
    const endIdx = Math.min(startIdx + filesPerPage, totalFiles);
    const paginatedFiles = filesForOutput.slice(startIdx, endIdx);

    const { finalFiles, paginationMetadata } = applyCharPagination(
      paginatedFiles,
      query.charOffset,
      query.charLength
    );

    const configFilePatterns =
      /\.(config|rc|env|json|ya?ml|toml|ini)$|^(\..*rc|config\.|\.env)/i;
    const hasConfigFiles = finalFiles.some(f =>
      configFilePatterns.test(f.path.split('/').pop() || '')
    );

    // Surface find stderr (e.g. permission-denied sub-directory warnings).
    // find exits 0 even when individual paths are inaccessible, so these
    // warnings would otherwise be silently swallowed.
    const findStderrWarnings: string[] = [];
    if (result.stderr?.trim()) {
      const stderrLines = result.stderr
        .trim()
        .split('\n')
        .map(l => l.replace(/^find:\s*/i, '').trim())
        .filter(Boolean);
      if (stderrLines.length > 0) {
        findStderrWarnings.push(...stderrLines.slice(0, 5));
        if (stderrLines.length > 5) {
          findStderrWarnings.push(
            `... and ${stderrLines.length - 5} more find warning(s)`
          );
        }
      }
    }

    const allWarnings = [...timeFormatWarnings, ...findStderrWarnings];

    const fullResult: LocalFindFilesToolResult = {
      // status omitted on success (absent ≡ "hasResults"); 'empty' is set
      // explicitly below when totalFiles === 0.
      ...(totalFiles === 0 ? { status: 'empty' as const } : {}),
      files: finalFiles,
      pagination: {
        currentPage: filePageNumber,
        totalPages,
        filesPerPage,
        totalFiles,
        hasMore: filePageNumber < totalPages,
      },
      ...(paginationMetadata && {
        charPagination: createPaginationInfo(paginationMetadata),
      }),
      ...(allWarnings.length > 0 && { warnings: allWarnings }),
      hints: buildFindFilesHints({
        query,
        filePageNumber,
        totalPages,
        shownCount: finalFiles.length,
        totalFiles,
        wasFileCapped,
        maxFiles,
        discoveredFileCount,
        hasConfigFiles,
        extraHints: sortHints,
        paginationMetadata,
      }),
    };

    return attachRawResponseChars(
      applyFindFilesVerbosity(fullResult, query, { totalFiles }),
      result.stdout.length
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
    }) as LocalFindFilesToolResult;
  }
}

/**
 * Concise payload is a one-line summary with a `newest:` drill-back hint
 * pointing at the first file. Omitted / `"basic"` / `"compact"` all preserve
 * `files[]` here; compact's hint-trim is a follow-up (Task #8).
 *
 * Exported for direct unit testing in `tests/scheme/verbosity_concise.test.ts`.
 */
export function applyFindFilesVerbosity(
  result: LocalFindFilesToolResult,
  query: FindFilesQuery,
  totals: { totalFiles: number }
): LocalFindFilesToolResult {
  if (isConcise(query.verbosity)) {
    // hasResults ≡ absent status; only 'empty'/'error' carry a marker.
    if (result.status !== undefined) return result;
    const topFile = result.files?.[0];
    const newest = topFile?.path ?? '';
    const dirs = new Set(
      (result.files ?? []).map(f => f.path.split('/').slice(0, -1).join('/'))
    );
    const summary =
      `${totals.totalFiles} files in ${dirs.size} dirs` +
      (newest ? ` (newest: ${newest})` : '');
    return {
      ...result,
      files: [],
      hints: [summary],
    };
  }
  if (isCompact(query.verbosity)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryFindFilesHint, 2),
    };
  }
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
  showLastModified: boolean
): LocalFindFilesEntry[] {
  return files.map(f => {
    const result: LocalFindFilesEntry = { path: f.path, type: f.type };
    if (details) {
      // Emit both raw bytes (for tooling) and human-readable form (for the
      // agent). The dual emission costs ~20 bytes per file but lets
      // downstream consumers parse the raw number without re-parsing
      // "12.4KB" strings.
      if (f.size !== undefined) {
        result.size = f.size;
        result.sizeFormatted = formatFileSize(f.size);
      }
      if (f.permissions) result.permissions = f.permissions;
    }
    if (showLastModified && f.modified) {
      result.modified = f.modified;
    }
    return result;
  });
}

function applyCharPagination(
  paginatedFiles: LocalFindFilesEntry[],
  charOffset?: number,
  charLength?: number
): {
  finalFiles: LocalFindFilesEntry[];
  paginationMetadata: PaginationMetadata | null;
} {
  if (!charLength) {
    return { finalFiles: paginatedFiles, paginationMetadata: null };
  }

  const fullJson = serializeForPagination(paginatedFiles, false);
  const totalChars = fullJson.length;
  const startOffset = charOffset ?? 0;
  const targetLength = charLength;
  const endLimit = startOffset + targetLength;

  if (startOffset >= totalChars) {
    return {
      finalFiles: [],
      paginationMetadata: {
        paginatedContent: '[]',
        byteOffset: startOffset,
        byteLength: 0,
        totalBytes: totalChars,
        charOffset: startOffset,
        charLength: 0,
        totalChars,
        hasMore: false,
        estimatedTokens: 0,
        currentPage: Math.floor(startOffset / targetLength) + 1,
        totalPages: Math.ceil(totalChars / targetLength),
      },
    };
  }

  const selectedFiles: LocalFindFilesEntry[] = [];
  let currentPos = 1;

  for (let i = 0; i < paginatedFiles.length; i++) {
    const itemLen = JSON.stringify(paginatedFiles[i]).length;
    const itemStart = currentPos;
    const itemEnd = itemStart + itemLen;

    if (itemStart < endLimit && itemEnd > startOffset) {
      selectedFiles.push(paginatedFiles[i]!);
    }
    currentPos += itemLen + 1;
    if (currentPos >= endLimit) break;
  }

  const finalJson = serializeForPagination(selectedFiles, false);
  const hasMore = currentPos < totalChars;

  return {
    finalFiles: selectedFiles,
    paginationMetadata: {
      paginatedContent: finalJson,
      byteOffset: startOffset,
      byteLength: finalJson.length,
      totalBytes: totalChars,
      charOffset: startOffset,
      charLength: finalJson.length,
      totalChars,
      hasMore,
      nextCharOffset: hasMore ? currentPos : undefined,
      estimatedTokens: Math.ceil(finalJson.length / 4),
      currentPage: Math.floor(startOffset / targetLength) + 1,
      totalPages: Math.ceil(totalChars / targetLength),
    },
  };
}

async function getFileDetails(
  filePaths: string[],
  showModified: boolean = false
): Promise<LocalFindFilesEntry[]> {
  const CONCURRENCY_LIMIT = 24;

  const results: LocalFindFilesEntry[] = new Array(filePaths.length);

  const processAtIndex = async (index: number) => {
    const filePath = filePaths[index]!;
    try {
      const stats = await fs.promises.lstat(filePath);

      let type: 'file' | 'directory' | 'symlink' = 'file';
      if (stats.isDirectory()) type = 'directory';
      else if (stats.isSymbolicLink()) type = 'symlink';

      const file: LocalFindFilesEntry = {
        path: filePath,
        type,
        size: stats.size,
        permissions: stats.mode.toString(8).slice(-3),
      };
      if (showModified) {
        file.modified = stats.mtime.toISOString();
      }
      results[index] = file;
    } catch {
      results[index] = {
        path: filePath,
        type: 'file',
      };
    }
  };

  let nextIndex = 0;
  const getNext = () => {
    const current = nextIndex;
    nextIndex += 1;
    return current < filePaths.length ? current : -1;
  };
  const worker = async () => {
    for (let i = getNext(); i !== -1; i = getNext()) {
      await processAtIndex(i);
    }
  };

  const workers = Array.from(
    { length: Math.min(CONCURRENCY_LIMIT, filePaths.length) },
    () => worker()
  );
  await Promise.all(workers);

  return results;
}

const VALID_TIME_STRING_RE = /^\d+[hdwm]$/;

/**
 * Validate time-filter fields that are passed to `find -mtime / -mmin`.
 * Only relative durations like "7d", "2h", "1w", "3m" are supported.
 * ISO timestamps (e.g. "2026-06-01T00:00:00Z") are NOT supported and
 * would silently produce -mtime -0 / +0 (match nothing / match everything).
 * This function surfaces an explicit warning for each invalid field.
 */
function validateTimeFilterFormats(query: FindFilesQuery): string[] {
  const warnings: string[] = [];
  const fields: Array<{ key: string; value: string | undefined }> = [
    {
      key: 'modifiedAfter',
      value: (query as Record<string, unknown>).modifiedAfter as
        | string
        | undefined,
    },
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
