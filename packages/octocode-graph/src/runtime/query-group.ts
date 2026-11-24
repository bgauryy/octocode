// `--count` and `--group <key>` over any answer whose rows name files: the count, or one row per group with its file
// count and bytes, instead of every row. The op's own answer (summary, stats, highlight) stays; only rows fold.
import type { QueryIndex } from './query-index.ts';
import {
  plural,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';

/** Bytes of a file as the build checked it on disk; null when the export has no check for it. */
export const bytesOf = (I: QueryIndex, id: string): number | null =>
  I.file.get(id)?.check?.bytes ?? null;

/** A byte count for people: B, KiB or MiB; `size unknown` for null. JSON keeps the exact bytes. */
export function formatBytes(n: number | null | undefined) {
  if (n == null) return 'size unknown';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(2)} MiB`;
}

/** The workspace package a file belongs to: its chunk's package, else the chunk, else its top folder. */
export function packageOf(I: QueryIndex, id: string) {
  const file = I.file.get(id);
  if (file?.kind === 'package' || id.startsWith('pkg:')) return id;
  const chunk = I.D.chunks?.find(c => c.id === file?.chunk);
  return chunk?.package
    ? `pkg:${chunk.package}`
    : (file?.chunk ?? id.split('/')[0]);
}

function keyOf(I: QueryIndex, row: Row, by: string) {
  if (by === 'depth') return row.depth == null ? '-' : String(row.depth);
  if (by === 'chunk') return row.chunk ?? I.file.get(row.id)?.chunk ?? '-';
  if (by === 'package') return packageOf(I, row.id);
  const dirs = row.id.startsWith('pkg:')
    ? [row.id]
    : row.id.split('/').slice(0, -1);
  const n = by.startsWith('dir:') ? Number(by.slice(4)) : dirs.length;
  return dirs.slice(0, n).join('/') || '.';
}

/** The answer with its rows counted (`--count`) or folded into groups (`--group`); unchanged without either. */
export function grouped(I: QueryIndex, r: OpResult, q: QueryArgs): OpResult {
  if (!q.count && !q.group) return r;
  const rows = r.rows.filter(row => typeof row.id === 'string');
  if (!q.group)
    return {
      ...r,
      rows: [],
      count: rows.length,
      summary: `${r.summary} [${plural(rows.length, 'row')}; rows left out by --count]`,
    };
  const groups = new Map<
    string,
    { files: number; bytes: number; known: number }
  >();
  for (const row of rows) {
    const key = keyOf(I, row, q.group);
    const g = groups.get(key) ?? { files: 0, bytes: 0, known: 0 };
    g.files++;
    const bytes = bytesOf(I, row.id);
    if (bytes != null) {
      g.bytes += bytes;
      g.known++;
    }
    groups.set(key, g);
  }
  const folded: Row[] = [...groups]
    .toSorted(([a, x], [b, y]) => y.files - x.files || (a < b ? -1 : 1))
    .map(([key, g]) => ({
      id: key,
      label: key,
      loc: null,
      group: q.group,
      count: g.files,
      // Bytes only when every file in the group has its size: a partial sum would read as the total.
      bytes: g.known === g.files ? g.bytes : null,
    }));
  return {
    ...r,
    rows: folded,
    count: rows.length,
    summary: `${r.summary} [${plural(rows.length, 'row')} in ${plural(folded.length, 'group')} by ${q.group}]`,
  };
}
