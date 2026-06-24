/**
 * Map typed tool results into OQL result rows + pagination. Pure functions —
 * no I/O — so they are unit-testable without backends.
 */
import type {
  LocalFindFilesToolResult,
  LocalGetFileContentToolResult,
  LocalSearchCodeToolResult,
  LocalViewStructureToolResult,
} from '@octocodeai/octocode-core/extra-types';
import type {
  OqlCodeResultRow,
  OqlContentResultRow,
  OqlFileResultRow,
  OqlResultRow,
  OqlTreeResultRow,
  Pagination,
  QuerySource,
} from '../types.js';
import { toOqlPagination, type ToolPaginationPayload } from './pagination.js';

export interface MappedResult {
  results: OqlResultRow[];
  pagination?: Pagination;
}

// Delegate to the single OQL pagination normalizer so the code/structure/file
// paths recompute `totalPages`/`hasMore` from the resolved unit fields exactly
// like every other target (P2 — one consistent page math everywhere).
function toPagination(
  p: ToolPaginationPayload | undefined
): Pagination | undefined {
  return toOqlPagination(p);
}

function toCodePagination(
  p: ToolPaginationPayload | undefined
): Pagination | undefined {
  if (!p) return undefined;

  // localSearchCode pages matched files, while OQL code rows are individual
  // matches. Keep that unit explicit so the runner does not invent row-page
  // continuations that localSearchCode cannot execute.
  return toOqlPagination({
    currentPage: p.currentPage,
    totalPages: p.totalPages,
    nextPage: p.nextPage,
    hasMore: p.hasMore,
    itemsPerPage: p.filesPerPage ?? p.itemsPerPage ?? p.perPage,
    totalItems: p.totalFiles ?? p.totalItems,
    reportedTotalMatches: p.totalMatches ?? p.reportedTotalMatches,
    reachableTotalMatches: p.reachableTotalMatches,
    totalMatchesKind: p.totalFiles !== undefined ? 'files' : p.totalMatchesKind,
    totalMatchesCapped: p.totalMatchesCapped,
    uniqueFileCount: p.uniqueFileCount ?? p.totalFiles,
  });
}

export function mapCodeResult(
  result: LocalSearchCodeToolResult,
  source: QuerySource
): MappedResult {
  const rows: OqlCodeResultRow[] = [];
  for (const file of result.files ?? []) {
    const matches = file.matches ?? [];
    if (matches.length === 0) {
      // filesOnly / discovery / count modes: one row per file at line 1.
      // Count modes carry file-level totals even though they do not carry match rows.
      const counts = file as {
        totalMatchedLines?: number;
        totalOccurrences?: number;
      };
      rows.push({
        kind: 'code',
        source,
        path: file.path,
        line: 1,
        ...(counts.totalMatchedLines !== undefined
          ? { totalMatchedLines: counts.totalMatchedLines }
          : {}),
        ...(counts.totalOccurrences !== undefined
          ? { totalOccurrences: counts.totalOccurrences }
          : {}),
      });
      continue;
    }
    for (const m of matches) {
      // Structural metavariable captures ($X, $$$ARGS) and their precise source
      // ranges. The engine produces these for structural matches that capture;
      // forward them verbatim (never fabricated when absent). The per-capture
      // ranges let an agent feed a capture straight to lspGetSemantics.
      const captures = m.metavars;
      const ranges = m.metavarRanges;
      rows.push({
        kind: 'code',
        source,
        path: file.path,
        line: m.line,
        ...(m.endLine !== undefined ? { endLine: m.endLine } : {}),
        ...(m.column !== undefined ? { column: m.column } : {}),
        ...(m.value !== undefined ? { snippet: m.value } : {}),
        ...(captures && Object.keys(captures).length
          ? { metavars: captures }
          : {}),
        ...(ranges && Object.keys(ranges).length
          ? { metavarRanges: ranges }
          : {}),
      });
    }
  }
  return {
    results: rows,
    pagination: enrichCodePagination(
      toCodePagination(result.pagination as Parameters<typeof toPagination>[0]),
      rows.length
    ),
  };
}

function enrichCodePagination(
  pagination: Pagination | undefined,
  rowCount: number
): Pagination | undefined {
  if (!pagination) return undefined;
  if (pagination.totalItemsKind !== 'files') return pagination;
  return {
    ...pagination,
    itemUnit: 'files',
    rowCount,
    ...(pagination.reportedTotalItems !== undefined
      ? { reportedRowCount: pagination.reportedTotalItems }
      : {}),
  };
}

export function mapFilesResult(
  result: LocalFindFilesToolResult,
  source: QuerySource
): MappedResult {
  const rows: OqlFileResultRow[] = (result.files ?? []).map(entry => {
    const t = entry.type;
    const entryType: 'file' | 'directory' =
      t === 'd' || t === 'directory' ? 'directory' : 'file';
    return {
      kind: 'file',
      source,
      path: entry.path,
      entryType,
      ...(entry.size !== undefined ? { size: entry.size } : {}),
      ...(entry.modified !== undefined ? { modified: entry.modified } : {}),
    };
  });
  return {
    results: rows,
    pagination: toPagination(
      result.pagination as Parameters<typeof toPagination>[0]
    ),
  };
}

export function mapStructureResult(
  result: LocalViewStructureToolResult,
  source: QuerySource
): MappedResult {
  // Grouped-list fallback (when the tool emits files/folders arrays instead of
  // per-entry rows, e.g. non-detail mode).
  if (!result.entries && (result.files || result.folders)) {
    const base = result.path ?? '';
    const join = (name: string) => `${base.replace(/\/$/, '')}/${name}`;
    const rows: OqlTreeResultRow[] = [
      ...(result.folders ?? []).map(name => ({
        kind: 'tree' as const,
        source,
        path: join(name),
        entryType: 'directory' as const,
        depth: 0,
      })),
      ...(result.files ?? []).map(name => ({
        kind: 'tree' as const,
        source,
        path: join(name),
        entryType: 'file' as const,
        depth: 0,
      })),
    ];
    return {
      results: rows,
      pagination: toPagination(
        result.pagination as Parameters<typeof toPagination>[0]
      ),
    };
  }
  const rows: OqlTreeResultRow[] = (result.entries ?? []).map(entry => {
    const t = entry.type;
    const entryType: 'file' | 'directory' =
      t === 'dir' || t === 'directory' ? 'directory' : 'file';
    // viewStructure reports size as a formatted string ("5.7KB"); the OQL tree
    // row size is bytes (number), so coerce.
    const size = coerceSizeToBytes(entry.size);
    return {
      kind: 'tree',
      source,
      path: entry.path ?? entry.name ?? '',
      entryType,
      depth: entry.depth ?? 0,
      ...(size !== undefined ? { size } : {}),
    };
  });
  return {
    results: rows,
    pagination: toPagination(
      result.pagination as Parameters<typeof toPagination>[0]
    ),
  };
}

const SIZE_UNITS: Record<string, number> = {
  b: 1,
  kb: 1024,
  mb: 1024 ** 2,
  gb: 1024 ** 3,
  tb: 1024 ** 4,
};

/** Coerce a numeric or formatted ("5.7KB") size to bytes; undefined if absent. */
function coerceSizeToBytes(
  size: number | string | undefined
): number | undefined {
  if (typeof size === 'number') return size;
  if (typeof size !== 'string') return undefined;
  const m = size.trim().match(/^([0-9.]+)\s*([kmgt]?b)$/i);
  if (!m) {
    const n = Number(size);
    return Number.isFinite(n) ? n : undefined;
  }
  const value = Number(m[1]);
  const unit = SIZE_UNITS[m[2]!.toLowerCase()] ?? 1;
  return Number.isFinite(value) ? Math.round(value * unit) : undefined;
}

export function mapContentResult(
  result: LocalGetFileContentToolResult,
  source: QuerySource,
  path: string,
  /**
   * The view the OQL query *requested*. The backing tool does not reliably echo
   * the minify mode back (e.g. a `symbols` read reports `standard`), so the row
   * must report the requested view to satisfy "report the view used".
   */
  requestedView: OqlContentResultRow['contentView'] = 'compact'
): MappedResult {
  // CharPagination (char window) carries charOffset/charLength/totalChars; a
  // plain PaginationInfo does not. Detect by presence of charOffset.
  const pag = result.pagination as
    | {
        hasMore?: boolean;
        charOffset?: number;
        charLength?: number;
        totalChars?: number;
      }
    | undefined;
  const hasCharWindow = typeof pag?.charOffset === 'number';

  const range: NonNullable<OqlContentResultRow['range']> = {
    ...(result.startLine !== undefined ? { startLine: result.startLine } : {}),
    ...(result.endLine !== undefined ? { endLine: result.endLine } : {}),
    ...(hasCharWindow
      ? {
          charOffset: pag!.charOffset,
          ...(typeof pag!.charLength === 'number'
            ? { charLength: pag!.charLength }
            : {}),
        }
      : {}),
  };

  const row: OqlContentResultRow = {
    kind: 'content',
    source,
    path: result.filePath ?? path,
    content: result.content ?? '',
    contentView: requestedView,
    ...(Object.keys(range).length ? { range } : {}),
  };
  return {
    results: [row],
    ...(pag?.hasMore !== undefined
      ? { pagination: { hasMore: Boolean(pag.hasMore) } }
      : {}),
  };
}
