/**
 * Boolean (`all`/`any`/`not`) predicate evaluation over the FILES target,
 * with file-path set algebra: all=intersection, any=union, not=universe−set.
 * Local scope IS the complete candidate universe, so negation is exact (no
 * negativeUniverseRequired). Each leaf runs as its own backend call — the
 * attribute (field-predicate) lookups live in filesUniverse.ts, the content
 * (text/regex/structural) lookups live in filesContentLeaf.ts — this module
 * only dispatches between them and combines the resulting row sets.
 */
import { diagnostic } from '../../diagnostics.js';
import { universeFileRows, leafFieldFileRows } from './filesUniverse.js';
import { leafContentFileRows } from './filesContentLeaf.js';
import { intersectFileRows, mergeFileRows } from './fileRowOps.js';
import type {
  OqlDiagnostic,
  OqlFileResultRow,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QuerySource,
} from '../../types.js';
import type { AdapterResult, FileRowMap } from './types.js';

/**
 * Evaluate a boolean `where` over the FILES target with file-level set algebra.
 */
export async function executeFilesBoolean(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  where: Predicate
): Promise<AdapterResult> {
  const provenanceOut: OqlProvenance[] = [];
  const diagnostics: OqlDiagnostic[] = [];
  const set = await evalFilesPredicate(
    query,
    where,
    source,
    searchPath,
    provenanceOut,
    diagnostics
  );
  const rows = [...set.values()].sort(fileRowComparator(query));
  if (rows.length === 0 && !diagnostics.some(d => d.severity === 'error')) {
    diagnostics.push(
      diagnostic('zeroMatches', 'Boolean file query matched no files.', {
        backend: 'localSearchCode',
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  return { results: rows, diagnostics, provenance: provenanceOut };
}

// Order boolean-combined file rows like the attribute lookup would: size and
// modified descending (largest/newest first), name/path ascending. Set
// algebra loses backend ordering, so the sort re-applies here.
function fileRowComparator(
  query: OqlQuery
): (a: OqlFileResultRow, b: OqlFileResultRow) => number {
  const sort = query.controls?.search?.sort;
  const byPath = (a: OqlFileResultRow, b: OqlFileResultRow) =>
    a.path.localeCompare(b.path);
  if (sort === 'size') {
    return (a, b) => (b.size ?? 0) - (a.size ?? 0) || byPath(a, b);
  }
  if (sort === 'modified') {
    return (a, b) =>
      (b.modified ?? '').localeCompare(a.modified ?? '') || byPath(a, b);
  }
  if (sort === 'name') {
    const base = (p: string) => p.slice(p.lastIndexOf('/') + 1);
    return (a, b) => base(a.path).localeCompare(base(b.path)) || byPath(a, b);
  }
  return byPath;
}

async function evalFilesPredicate(
  query: OqlQuery,
  p: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<FileRowMap> {
  switch (p.kind) {
    case 'all': {
      const sets = await Promise.all(
        p.of.map(c =>
          evalFilesPredicate(query, c, source, searchPath, prov, diags)
        )
      );
      return sets.reduce((acc, s) =>
        acc === undefined ? s : intersectFileRows(acc, s)
      );
    }
    case 'any': {
      const sets = await Promise.all(
        p.of.map(c =>
          evalFilesPredicate(query, c, source, searchPath, prov, diags)
        )
      );
      const out: FileRowMap = new Map();
      for (const s of sets) {
        for (const [path, row] of s) {
          out.set(path, mergeFileRows(out.get(path), row));
        }
      }
      return out;
    }
    case 'not': {
      const universe = await universeFileRows(query, source, searchPath, prov);
      const inner = await evalFilesPredicate(
        query,
        p.predicate,
        source,
        searchPath,
        prov,
        diags
      );
      return new Map([...universe].filter(([path]) => !inner.has(path)));
    }
    default:
      return leafFileRows(query, p, source, searchPath, prov, diags);
  }
}

async function leafFileRows(
  query: OqlQuery,
  leaf: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<FileRowMap> {
  if (leaf.kind === 'field') {
    return leafFieldFileRows(query, leaf, source, searchPath, prov, diags);
  }
  return leafContentFileRows(query, leaf, source, searchPath, prov, diags);
}

/** All files in scope — the candidate universe for negation. */
export async function universeFiles(
  query: OqlQuery,
  searchPath: string,
  prov: OqlProvenance[]
): Promise<Set<string>> {
  return new Set(
    (
      await universeFileRows(query, query.from as QuerySource, searchPath, prov)
    ).keys()
  );
}

/** File-set for a single (non-boolean) predicate. */
export async function leafFiles(
  query: OqlQuery,
  leaf: Predicate,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<Set<string>> {
  return new Set(
    (
      await leafFileRows(
        query,
        leaf,
        query.from as QuerySource,
        searchPath,
        prov,
        diags
      )
    ).keys()
  );
}
