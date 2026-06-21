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

export interface MappedResult {
  results: OqlResultRow[];
  pagination?: Pagination;
}

function toPagination(
  p:
    | { currentPage?: number; totalPages?: number; hasMore?: boolean }
    | undefined
): Pagination | undefined {
  if (!p) return undefined;
  return {
    ...(p.currentPage !== undefined ? { currentPage: p.currentPage } : {}),
    ...(p.totalPages !== undefined ? { totalPages: p.totalPages } : {}),
    hasMore: Boolean(p.hasMore),
  };
}

export function mapCodeResult(
  result: LocalSearchCodeToolResult,
  source: QuerySource
): MappedResult {
  const rows: OqlCodeResultRow[] = [];
  for (const file of result.files ?? []) {
    const matches = file.matches ?? [];
    if (matches.length === 0) {
      // filesOnly / discovery mode: one row per file at line 1
      rows.push({ kind: 'code', source, path: file.path, line: 1 });
      continue;
    }
    for (const m of matches) {
      rows.push({
        kind: 'code',
        source,
        path: file.path,
        line: m.line,
        ...(m.column !== undefined ? { column: m.column } : {}),
        ...(m.value !== undefined ? { snippet: m.value } : {}),
      });
    }
  }
  return {
    results: rows,
    pagination: toPagination(
      result.pagination as Parameters<typeof toPagination>[0]
    ),
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
  path: string
): MappedResult {
  const viewMap: Record<string, OqlContentResultRow['contentView']> = {
    none: 'exact',
    standard: 'compact',
    symbols: 'symbols',
  };
  const row: OqlContentResultRow = {
    kind: 'content',
    source,
    path: result.filePath ?? path,
    content: result.content ?? '',
    contentView: viewMap[result.contentView ?? 'standard'] ?? 'compact',
    ...(result.startLine !== undefined || result.endLine !== undefined
      ? {
          range: {
            ...(result.startLine !== undefined
              ? { startLine: result.startLine }
              : {}),
            ...(result.endLine !== undefined
              ? { endLine: result.endLine }
              : {}),
          },
        }
      : {}),
  };
  const charPag = result.pagination as { hasMore?: boolean } | undefined;
  return {
    results: [row],
    ...(charPag?.hasMore !== undefined
      ? { pagination: { hasMore: Boolean(charPag.hasMore) } }
      : {}),
  };
}
