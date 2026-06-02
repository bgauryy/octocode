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
import type { z } from 'zod/v4';
import type { ViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';

type UpstreamViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithOptionalMeta } from '../../types/execution.js';

/**
 * Handler-side query type: upstream input shape plus the overlay's `verbosity`
 * field. Augmenting locally (vs. importing the overlay's output type alias)
 * preserves the existing input-shape contract — callers that pass partial
 * queries continue to type-check while the handler still sees `verbosity`.
 */
type ViewStructureQuery = WithVerbosity<
  WithOptionalMeta<UpstreamViewStructureQuery>
>;

function buildActiveViewStructureFilters(query: ViewStructureQuery): string[] {
  const activeFilters: string[] = [`path: ${query.path}`];
  if (query.depth !== undefined) activeFilters.push(`depth: ${query.depth}`);
  if (query.extension) activeFilters.push(`extension: ${query.extension}`);
  if (query.extensions?.length) {
    activeFilters.push(`extensions: ${query.extensions.join(', ')}`);
  }
  if (query.pattern) activeFilters.push(`pattern: ${query.pattern}`);
  if (query.filesOnly) activeFilters.push('filesOnly');
  if (query.directoriesOnly) activeFilters.push('directoriesOnly');
  if (query.hidden) activeFilters.push('hidden');
  return [`Active filters — ${activeFilters.join(' | ')}`];
}

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
      query as { itemsPerPage?: number; page?: number }
    );
    const sanitizedBasePath = pathValidation.sanitizedPath!;
    const outputEntries = paginatedEntries.map(entry => ({
      ...toEntryObject(entry),
      path: `${sanitizedBasePath}/${entry.name}`,
    }));
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
      applyViewStructureVerbosity(
        {
          ...(isEmpty ? { status: 'empty' as const } : {}),
          entries: outputEntries,
          summary,
          pagination,
          ...(warnings.length > 0 && { warnings }),
          hints: [
            ...buildActiveViewStructureFilters(query),
            ...entryPaginationHints,
            ...(isEmpty
              ? getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, 'empty', {
                  entryCount: totalEntries,
                  path: query.path,
                  extension: query.extension,
                  pattern:
                    typeof (query as { pattern?: unknown }).pattern === 'string'
                      ? (query as { pattern?: string }).pattern
                      : undefined,
                } as Record<string, unknown>)
              : []),
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
        ? [`Path not found: ${basePath}`]
        : [`Permission denied: ${basePath}`],
    }) as LocalViewStructureToolResult;
  }

  let filteredEntries = applyEntryFilters(entries, query);

  if (query.sortBy) {
    filteredEntries = filteredEntries.sort((a, b) => {
      let comparison = 0;
      switch (query.sortBy) {
        case 'size': {
          // Use raw byte count to avoid parseFileSize round-trip loss on
          // the formatted size string (e.g. "12.4KB" → parse → float).
          const aSize = a.sizeBytes ?? (a.size ? parseFileSize(a.size) : 0);
          const bSize = b.sizeBytes ?? (b.size ? parseFileSize(b.size) : 0);
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
    query as { itemsPerPage?: number; page?: number }
  );
  const outputEntries = paginatedEntries.map(entry => ({
    ...toEntryObject(entry),
    path: `${basePath}/${entry.name}`,
  }));
  const warnings = buildWalkWarnings(walkStats);
  const isEmpty = totalEntries === 0;
  const baseHints = isEmpty
    ? getHints(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, 'empty')
    : [];
  const entryPaginationHints = buildEntryPaginationHints(
    filteredEntries,
    paginatedEntries.length,
    pagination,
    endIdx
  );
  const summary = summarizeEntries(filteredEntries);

  return attachRawResponseChars(
    applyViewStructureVerbosity(
      {
        // status omitted on success (absent ≡ "hasResults"); 'empty' set
        // explicitly when totalEntries === 0.
        ...(isEmpty ? { status: 'empty' as const } : {}),
        entries: outputEntries,
        summary,
        pagination,
        ...(warnings.length > 0 && { warnings }),
        hints: [
          ...buildActiveViewStructureFilters(query),
          ...baseHints,
          ...entryPaginationHints,
        ],
      },
      query
    ),
    countSerializedChars(entries)
  );
}

/** How many entry names concise samples into the `top:` hint for drill-down. */
const CONCISE_TOP_ENTRIES = 5;

/**
 * Predicate identifying advisory hints this tool emits — recovery prose,
 * monorepo suggestions, large-tree warnings. Stripped under `compact`.
 * Substring-OR, case-insensitive.
 */
const isAdvisoryViewStructureHint = makeAdvisoryPredicate([
  'monorepo',
  'workspace root',
  'auto-excludes',
  'large tree',
  'large payload',
  'large directory',
]);

/**
 * Shape the result for the requested verbosity.
 *
 * - concise: drop `entries[]`; keep `summary` + `pagination` so the agent still
 *   sees `totalEntries`. No verbosity-feature hints are emitted.
 * - compact: trim advisory hints via `compactTrimHints()`; `entries[]`
 *   unchanged.
 * - omitted / basic: passthrough.
 */
export function applyViewStructureVerbosity(
  result: LocalViewStructureToolResult,
  query: ViewStructureQuery
): LocalViewStructureToolResult {
  if (isConcise(query.verbosity)) {
    // hasResults ≡ absent status; only 'empty'/'error' carry a marker.
    if (result.status !== undefined) return result;
    // Drop entries[] but keep concise research-grade: emit the count summary
    // PLUS a sample of top entry names so the agent has a concrete path to
    // drill into. A bare count is a dead-end; names give the next move.
    const names = (result.entries ?? [])
      .slice(0, CONCISE_TOP_ENTRIES)
      .map(e => e.name)
      .filter(Boolean);
    const total =
      result.pagination?.totalEntries ?? result.entries?.length ?? 0;
    const more = total > names.length ? ` (+${total - names.length} more)` : '';
    const hints: string[] = [];
    if (result.summary) hints.push(`summary: ${result.summary}`);
    if (names.length > 0) hints.push(`top: ${names.join(', ')}${more}`);
    return { ...result, entries: [], hints };
  }
  if (isCompact(query.verbosity)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryViewStructureHint, 2),
    };
  }
  return result;
}
