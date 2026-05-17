import { LsCommandBuilder } from '../../commands/LsCommandBuilder.js';
import { parseFileSize } from '../../utils/file/size.js';
import { safeExec } from '../../utils/exec/safe.js';
import {
  checkCommandAvailability,
  getMissingCommandError,
} from '../../utils/exec/commandAvailability.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type {
  LocalViewStructureToolResult,
  ViewStructureQuery as UpstreamViewStructureQuery,
} from '@octocodeai/octocode-core';
import type { Verbosity } from '../../scheme/localSchemaOverlay.js';

/**
 * Handler-side query type: upstream input shape plus the overlay's `verbosity`
 * field. Augmenting locally (vs. importing the overlay's output type alias)
 * preserves the existing input-shape contract — callers that pass partial
 * queries continue to type-check while the handler still sees `verbosity`.
 */
type ViewStructureQuery = UpstreamViewStructureQuery & {
  verbosity?: Verbosity;
};
import { ToolErrors } from '../../errors/errorFactories.js';
import {
  applyEntryFilters,
  toEntryObject,
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

    // For recursive mode, we use Node.js fs directly (no external command needed)
    const effectiveShowModified = query.showFileLastModified ?? true;

    if (query.depth || query.recursive) {
      return await viewStructureRecursive(
        query,
        pathValidation.sanitizedPath!,
        effectiveShowModified
      );
    }

    // For non-recursive mode, check if ls is available
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
        path: pathValidation.sanitizedPath!,
      })
      .build();

    const result = await safeExec(command, args);

    if (!result.success) {
      const stderrMsg = result.stderr?.trim();
      const toolError = ToolErrors.commandExecutionFailed(
        'ls',
        new Error(stderrMsg || 'Unknown error'),
        stderrMsg
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        customHints: stderrMsg
          ? [`Error: ${stderrMsg}`]
          : ['ls command failed'],
        rawResponse: result.stdout.length + result.stderr.length,
      }) as LocalViewStructureToolResult;
    }

    const entries = query.details
      ? parseLsLongFormat(result.stdout, effectiveShowModified)
      : await parseLsSimple(
          result.stdout,
          pathValidation.sanitizedPath!,
          effectiveShowModified
        );

    let filteredEntries = applyEntryFilters(entries, query);

    if (query.limit) {
      filteredEntries = filteredEntries.slice(0, query.limit);
    }

    const totalEntries = filteredEntries.length;
    const { paginatedEntries, endIdx, pagination } = paginateEntries(
      filteredEntries,
      query
    );
    const outputEntries = paginatedEntries.map(entry => toEntryObject(entry));
    const warnings: string[] = [];
    const status = totalEntries > 0 ? 'hasResults' : 'empty';
    const entryPaginationHints = buildEntryPaginationHints(
      filteredEntries,
      paginatedEntries.length,
      pagination,
      endIdx
    );
    const summary = summarizeEntries(filteredEntries);

    return attachRawResponseChars(
      applyVerbosity(
        {
          status,
          entries: outputEntries,
          summary,
          pagination,
          ...(warnings.length > 0 && { warnings }),
          hints: [
            ...entryPaginationHints,
            ...getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, status, {
              entryCount: totalEntries,
            }),
          ],
        },
        query.verbosity
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

  // Surface a clear error when the root path itself failed to open. This
  // replaces the misleading "N entries skipped due to permission errors"
  // warning that previously appeared for ENOENT or ENOTDIR failures.
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
        ? [
            'Path does not exist or is not a directory',
            'Verify the path using localFindFiles',
            'To read a file, use localGetFileContent instead',
          ]
        : ['Check file/directory permissions'],
    }) as LocalViewStructureToolResult;
  }

  let filteredEntries = applyEntryFilters(entries, query);

  if (query.sortBy) {
    filteredEntries = filteredEntries.sort((a, b) => {
      let comparison = 0;
      switch (query.sortBy) {
        case 'size': {
          // Use numeric comparison instead of string comparison
          const aSize = a.size ? parseFileSize(a.size) : 0;
          const bSize = b.size ? parseFileSize(b.size) : 0;
          comparison = aSize - bSize;
          break;
        }
        case 'time':
          if (showModified && a.modified && b.modified) {
            comparison = a.modified.localeCompare(b.modified);
          } else {
            // Fallback to name when modified is not available
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
  }

  if (query.limit) {
    filteredEntries = filteredEntries.slice(0, query.limit);
  }

  const totalEntries = filteredEntries.length;
  const { paginatedEntries, endIdx, pagination } = paginateEntries(
    filteredEntries,
    query
  );
  const outputEntries = paginatedEntries.map(entry => toEntryObject(entry));
  const warnings = buildWalkWarnings(walkStats);
  const status = totalEntries > 0 ? 'hasResults' : 'empty';
  const baseHints = getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, status);
  const entryPaginationHints = buildEntryPaginationHints(
    filteredEntries,
    paginatedEntries.length,
    pagination,
    endIdx
  );
  const summary = summarizeEntries(filteredEntries);

  return attachRawResponseChars(
    applyVerbosity(
      {
        status,
        entries: outputEntries,
        summary,
        pagination,
        ...(warnings.length > 0 && { warnings }),
        hints: [...baseHints, ...entryPaginationHints],
      },
      query.verbosity
    ),
    countSerializedChars(entries)
  );
}

/**
 * Shape the result for the requested verbosity.
 *
 * RFC §4.7.2 + §4.7.9: when the agent opts into `verbosity: "ultra"`, drop
 * `entries[]` and return the one-line `summary` only. `pagination` is kept so
 * the agent still sees `totalEntries` and can decide whether to drill in.
 *
 * Compact / verbose / omitted → byte-identical to today (§3.1 contract).
 */
function applyVerbosity(
  result: LocalViewStructureToolResult,
  verbosity: ViewStructureQuery['verbosity']
): LocalViewStructureToolResult {
  if (verbosity !== 'ultra') return result;
  if (result.status === 'error' || result.status === 'empty') return result;

  return {
    ...result,
    entries: [],
    hints: [
      `verbosity:"ultra" — entries[] dropped. summary: ${result.summary ?? ''}`,
      'Drill-back: re-call with verbosity:"compact" (default) to see entries; ' +
        'use entryPageNumber + entriesPerPage if there are many.',
    ],
  };
}
