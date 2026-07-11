/**
 * Boolean (`all`/`any`/`not`) predicate evaluation over `target:"code"`.
 * Match rows come only from positive (non-negated) content leaves; `not` and
 * `field` leaves contribute file-set constraints, not occurrences. Local
 * scope is the complete universe, so negation is exact.
 */
import { searchContentRipgrep } from '../../../tools/local_ripgrep/searchContentRipgrep.js';
import { compileWhere } from '../compile.js';
import { mapCodeResult } from '../resultMap.js';
import { diagnostic } from '../../diagnostics.js';
import { scopeToCommon, applyLocalSearchLanguage } from './scope.js';
import { leafFiles, universeFiles } from './filesBoolean.js';
import { intersectStringSets } from './fileRowOps.js';
import type {
  OqlCodeResultRow,
  OqlDiagnostic,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QuerySource,
} from '../../types.js';
import type { AdapterResult, CodeEval } from './types.js';
import type { LocalSearchToolQuery } from './searchToolType.js';

export async function executeCodeBoolean(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  where: Predicate
): Promise<AdapterResult> {
  const prov: OqlProvenance[] = [];
  const diags: OqlDiagnostic[] = [];
  const evaluated = await evalCodePredicate(
    query,
    where,
    source,
    searchPath,
    prov,
    diags
  );
  // Restrict positive rows to the satisfying file set, dedup by path:line.
  const seen = new Set<string>();
  const rows = evaluated.rows
    .filter(r => evaluated.files.has(r.path))
    .filter(r => {
      const key = `${r.path}:${r.line ?? ''}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  if (rows.length === 0 && !diags.some(d => d.severity === 'error')) {
    diags.push(
      diagnostic('zeroMatches', 'Boolean code query matched no occurrences.', {
        backend: 'localSearchCode',
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  return { results: rows, diagnostics: diags, provenance: prov };
}

async function evalCodePredicate(
  query: OqlQuery,
  p: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<CodeEval> {
  switch (p.kind) {
    case 'all': {
      const parts = await Promise.all(
        p.of.map(c =>
          evalCodePredicate(query, c, source, searchPath, prov, diags)
        )
      );
      const files = parts
        .map(part => part.files)
        .reduce((acc, s) =>
          acc === undefined ? s : intersectStringSets(acc, s)
        );
      const rows = parts.flatMap(part => part.rows);
      return { files, rows };
    }
    case 'any': {
      const parts = await Promise.all(
        p.of.map(c =>
          evalCodePredicate(query, c, source, searchPath, prov, diags)
        )
      );
      const files = new Set<string>();
      for (const part of parts) for (const f of part.files) files.add(f);
      const rows = parts.flatMap(part => part.rows);
      return { files, rows };
    }
    case 'not': {
      const universe = await universeFiles(query, searchPath, prov);
      const inner = await evalCodePredicate(
        query,
        p.predicate,
        source,
        searchPath,
        prov,
        diags
      );
      // Negation yields a file-set constraint, no positive occurrences.
      return {
        files: new Set([...universe].filter(f => !inner.files.has(f))),
        rows: [],
      };
    }
    case 'field': {
      const files = await leafFiles(query, p, searchPath, prov, diags);
      return { files, rows: [] };
    }
    default: {
      // content leaf (text/regex/structural) -> match rows
      const rows = await leafCodeRows(
        query,
        p,
        source,
        searchPath,
        prov,
        diags
      );
      return { files: new Set(rows.map(r => r.path)), rows };
    }
  }
}

/** Match rows for a single content leaf (no boolean composition). */
async function leafCodeRows(
  query: OqlQuery,
  leaf: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<OqlCodeResultRow[]> {
  const compiled = compileWhere(leaf);
  if (compiled.unsupported) {
    diags.push(
      diagnostic(compiled.unsupported.code, compiled.unsupported.message, {
        backend: 'localSearchCode',
      })
    );
    return [];
  }
  const m = compiled.match!;
  const tq: Partial<LocalSearchToolQuery> = {
    path: searchPath,
    ...scopeToCommon(query.scope),
  };
  applyLocalSearchLanguage(tq, query.scope, m.langType);
  if (m.mode === 'structural') {
    tq.mode = 'structural';
    if (m.pattern !== undefined) tq.pattern = m.pattern;
    if (typeof m.rule === 'string') tq.rule = m.rule;
  } else {
    tq.keywords = m.keywords;
    if (m.fixedString) tq.fixedString = true;
    if (m.perlRegex) tq.perlRegex = true;
    if (m.caseSensitive) tq.caseSensitive = true;
    if (m.caseInsensitive) tq.caseInsensitive = true;
    if (m.wholeWord) tq.wholeWord = true;
    if (m.multiline) tq.multiline = true;
    if (m.multilineDotall) tq.multilineDotall = true;
  }
  const r = await searchContentRipgrep(tq as LocalSearchToolQuery);
  prov.push({ backend: 'localSearchCode', source });
  return mapCodeResult(r, source).results as OqlCodeResultRow[];
}
