import { parseFileSize } from '../../utils/file/size.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LsCommandBuilder } from '../../commands/LsCommandBuilder.js';
import {
  checkCommandAvailability,
  getMissingCommandError,
} from '../../utils/exec/commandAvailability.js';
import { safeExec } from '../../utils/exec/safe.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../../types/execution.js';
import type { ViewStructureQuery as LocalViewStructureQuery } from './scheme.js';

type ViewStructureQuery = WithOptionalMeta<LocalViewStructureQuery>;

/**
 * Sanitize raw `ls` stderr for agent-facing output: strip the `ls:` prefix
 * and redact absolute paths to a relative/short form (no filesystem leak).
 */
function sanitizeLsStderr(
  stderr: string | undefined,
  absolutePath: string
): string | undefined {
  const trimmed = stderr?.trim();
  if (!trimmed) return undefined;
  const redacted = redactPath(absolutePath);
  const sanitized = trimmed
    .split('\n')
    .map(line =>
      line
        .replace(/^ls:\s*/i, '')
        .split(absolutePath)
        .join(redacted)
    )
    .join('\n')
    .trim();
  return sanitized || undefined;
}

import { ToolErrors } from '../../errors/errorFactories.js';
import { redactPath } from '../../errors/pathUtils.js';
import {
  applyEntryFilters,
  toEntryObject,
  toGroupedLists,
  type DirectoryEntry,
} from './structureFilters.js';
import { parseLsSimple, parseLsLongFormat } from './structureParser.js';
import { walkDirectory, type WalkStats } from './structureWalker.js';
import {
  buildEntryPaginationHints,
  buildWalkWarnings,
  paginateEntries,
  summarizeEntries,
} from './structureResponse.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../utils/response/charSavings.js';

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

    // Lean by default: timestamps only on request, or when needed for
    // time-based sorting / detailed listings.
    const effectiveShowModified =
      query.showFileLastModified ??
      (query.sortBy === 'time' || query.details === true);

    if (query.depth || query.recursive) {
      return await viewStructureRecursive(
        query,
        pathValidation.sanitizedPath,
        effectiveShowModified
      );
    }

    const lsAvailability = await checkCommandAvailability('ls');
    if (!lsAvailability.available) {
      const toolError = ToolErrors.commandNotAvailable(
        'ls',
        getMissingCommandError('ls')
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      }) as LocalViewStructureToolResult;
    }

    const builder = new LsCommandBuilder();
    const { command, args } = builder
      .fromQuery({
        ...query,
        path: pathValidation.sanitizedPath,
      })
      .build();

    const result = await safeExec(command, args);

    if (!result.success) {
      const stderrMsg = sanitizeLsStderr(
        result.stderr,
        pathValidation.sanitizedPath
      );
      const toolError = ToolErrors.commandExecutionFailed(
        'ls',
        new Error(stderrMsg || 'Unknown error'),
        stderrMsg
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        customHints: [
          ...(stderrMsg ? [`Error: ${stderrMsg}`] : ['ls command failed']),
          'Verify the path exists — use localFindFiles or check ALLOWED_PATHS.',
        ],
        rawResponse: result.stdout.length + result.stderr.length,
      }) as LocalViewStructureToolResult;
    }

    const entries = query.details
      ? parseLsLongFormat(result.stdout, effectiveShowModified)
      : await parseLsSimple(
          result.stdout,
          pathValidation.sanitizedPath,
          effectiveShowModified
        );

    let filteredEntries = applyEntryFilters(entries, query);

    if (query.limit) {
      filteredEntries = filteredEntries.slice(0, query.limit);
    }

    const totalEntries = filteredEntries.length;
    const { paginatedEntries, endIdx, pagination } = paginateEntries(
      filteredEntries,
      query as { itemsPerPage?: number; page?: number }
    );
    const sanitizedBasePath = pathValidation.sanitizedPath;
    // Flat grouped name lists by default (githubViewRepoStructure parity);
    // rich per-entry objects when details or timestamps are requested.
    const richEntries =
      query.details === true || query.showFileLastModified === true;
    const entryPayload = richEntries
      ? {
          entries: paginatedEntries.map(entry => ({
            ...toEntryObject(entry),
            path: `${sanitizedBasePath}/${entry.name}`,
          })),
        }
      : { path: sanitizedBasePath, ...toGroupedLists(paginatedEntries) };
    const warnings: string[] = [];
    const isEmpty = totalEntries === 0;
    const entryPaginationHints = buildEntryPaginationHints(
      filteredEntries,
      paginatedEntries.length,
      pagination,
      endIdx
    );
    const summary = summarizeEntries(filteredEntries);

    return attachRawResponseChars(
      finalizeViewStructureResult(
        {
          ...(isEmpty ? { status: 'empty' as const } : {}),
          ...entryPayload,
          summary,
          // Suppress the pagination block on a single complete page — the
          // summary string already encodes the total count.
          ...(pagination.hasMore || pagination.totalPages > 1
            ? { pagination }
            : {}),
          ...(warnings.length > 0 && { warnings }),
          hints: [
            // Active-filters hint dropped — the agent set those params itself.
            ...(isEmpty
              ? getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, 'empty', {
                  entryCount: totalEntries,
                  path: query.path,
                  extensions: query.extensions,
                  pattern:
                    typeof (query as { pattern?: unknown }).pattern === 'string'
                      ? (query as { pattern?: string }).pattern
                      : undefined,
                } as Record<string, unknown>)
              : [
                  'Use localSearchCode to search or localGetFileContent to read discovered files.',
                ]),
            ...entryPaginationHints,
          ],
        },
        query
      ),
      result.stdout.length
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
      hints: getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, 'error'),
    };
  }
}

async function viewStructureRecursive(
  query: ViewStructureQuery,
  basePath: string,
  showModified: boolean = false
): Promise<LocalViewStructureToolResult> {
  const entries: DirectoryEntry[] = [];
  const maxDepth = query.depth || (query.recursive ? 5 : 2);

  const maxEntries = query.limit ? query.limit * 2 : 10000;

  const walkStats: WalkStats = { skipped: 0, permissionDenied: 0 };

  await walkDirectory({
    basePath,
    currentPath: basePath,
    depth: 0,
    maxDepth,
    entries,
    maxEntries,
    showHidden: query.hidden,
    showModified,
    stats: walkStats,
    showDetails: query.details ?? false,
  });

  if (walkStats.rootError) {
    const { code } = walkStats.rootError;
    const isNotFound = code === 'ENOENT' || code === 'ENOTDIR';
    const toolError = ToolErrors.pathValidationFailed(
      basePath,
      isNotFound
        ? `Directory not found: ${basePath}`
        : code === 'EACCES'
          ? `Permission denied: ${basePath}`
          : `Cannot access path: ${basePath}`
    );
    return createErrorResult(toolError, query, {
      toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      customHints: isNotFound
        ? [`Path not found: ${basePath}`]
        : [`Permission denied: ${basePath}`],
    }) as LocalViewStructureToolResult;
  }

  let filteredEntries = applyEntryFilters(entries, query);

  // Default to name sort so omitted sortBy matches the flat (ls) path's
  // alphabetical order instead of leaking filesystem traversal order.
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
  const { paginatedEntries, endIdx, pagination } = paginateEntries(
    filteredEntries,
    query as { itemsPerPage?: number; page?: number }
  );
  // Flat grouped name lists by default (githubViewRepoStructure parity);
  // rich per-entry objects when details or timestamps are requested.
  const richEntries =
    query.details === true || query.showFileLastModified === true;
  const entryPayload = richEntries
    ? {
        entries: paginatedEntries.map(entry => ({
          ...toEntryObject(entry),
          path: `${basePath}/${entry.name}`,
        })),
      }
    : { path: basePath, ...toGroupedLists(paginatedEntries) };
  const warnings = [
    ...buildWalkWarnings(walkStats),
    ...(walkStats.wasCapped
      ? [
          `Results capped at ${maxEntries} entries — add a pattern/extensions filter or reduce depth to narrow the scope.`,
        ]
      : []),
  ];
  const isEmpty = totalEntries === 0;
  const baseHints = isEmpty
    ? getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, 'empty', {
        entryCount: totalEntries,
        path: query.path,
        extensions: query.extensions,
        pattern:
          typeof (query as { pattern?: unknown }).pattern === 'string'
            ? (query as { pattern?: string }).pattern
            : undefined,
      } as Record<string, unknown>)
    : [
        'Use localSearchCode to search or localGetFileContent to read discovered files.',
      ];
  const entryPaginationHints = buildEntryPaginationHints(
    filteredEntries,
    paginatedEntries.length,
    pagination,
    endIdx
  );
  const summary = summarizeEntries(filteredEntries);

  return attachRawResponseChars(
    finalizeViewStructureResult(
      {
        ...(isEmpty ? { status: 'empty' as const } : {}),
        ...entryPayload,
        summary,
        // Suppress the pagination block on a single complete page — the
        // summary string already encodes the total count.
        ...(pagination.hasMore || pagination.totalPages > 1
          ? { pagination }
          : {}),
        ...(warnings.length > 0 && { warnings }),
        hints: [
          // Active-filters hint dropped — the agent set those params itself.
          ...baseHints,
          ...entryPaginationHints,
        ],
      },
      query
    ),
    countSerializedChars(entries)
  );
}

export function finalizeViewStructureResult(
  result: LocalViewStructureToolResult,
  _query: ViewStructureQuery
): LocalViewStructureToolResult {
  return result;
}
