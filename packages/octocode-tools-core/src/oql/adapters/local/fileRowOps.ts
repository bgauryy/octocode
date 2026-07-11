/**
 * Generic `FileRowMap` / string-set combinators used by the files-target
 * boolean set algebra (union/intersection/merge). Pure data-shape helpers —
 * no backend calls — shared by the field-leaf and content-leaf lookups.
 */
import type { OqlFileResultRow } from '../../types.js';
import type { FileRowMap } from './types.js';

export function mergeFileRows(
  left: OqlFileResultRow | undefined,
  right: OqlFileResultRow
): OqlFileResultRow {
  if (!left) return right;
  return {
    ...left,
    ...right,
    entryType:
      left.entryType === 'directory' || right.entryType === 'directory'
        ? 'directory'
        : 'file',
    ...(left.size !== undefined || right.size !== undefined
      ? { size: left.size ?? right.size }
      : {}),
    ...(left.modified !== undefined || right.modified !== undefined
      ? { modified: left.modified ?? right.modified }
      : {}),
  };
}

export function rowsByPath(rows: OqlFileResultRow[]): FileRowMap {
  const out: FileRowMap = new Map();
  for (const row of rows) {
    out.set(row.path, mergeFileRows(out.get(row.path), row));
  }
  return out;
}

export function intersectFileRows(a: FileRowMap, b: FileRowMap): FileRowMap {
  const out: FileRowMap = new Map();
  for (const [path, row] of a) {
    const other = b.get(path);
    if (other) out.set(path, mergeFileRows(row, other));
  }
  return out;
}

export function intersectStringSets(
  a: Set<string>,
  b: Set<string>
): Set<string> {
  return new Set([...a].filter(value => b.has(value)));
}
