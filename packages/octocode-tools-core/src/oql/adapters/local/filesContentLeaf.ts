/**
 * The content (ripgrep-backed) side of the files-target set algebra: a
 * single content-leaf lookup restricted to matching file paths, and the
 * "files without a content match" lane shared by the files and code targets.
 */
import { searchContentRipgrep } from '../../../tools/local_ripgrep/searchContentRipgrep.js';
import { compileWhere } from '../compile.js';
import { mapCodeResult } from '../resultMap.js';
import { diagnostic } from '../../diagnostics.js';
import {
  scopeToCommon,
  applyLocalSearchLanguage,
  searchControls,
  LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT,
} from './scope.js';
import { provenance, resultDiagnostics } from './predicates.js';
import { rowsByPath } from './fileRowOps.js';
import type {
  OqlDiagnostic,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QuerySource,
} from '../../types.js';
import type { AdapterResult, FileRowMap } from './types.js';
import type { LocalSearchToolQuery } from './searchToolType.js';

/** File-set for a single content-leaf (text/regex/structural) predicate. */
export async function leafContentFileRows(
  query: OqlQuery,
  leaf: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<FileRowMap> {
  const compiled = compileWhere(leaf);
  if (compiled.unsupported) {
    diags.push(
      diagnostic(compiled.unsupported.code, compiled.unsupported.message, {
        backend: 'localSearchCode',
      })
    );
    return new Map();
  }
  const m = compiled.match!;
  const tq: Partial<LocalSearchToolQuery> = {
    path: searchPath,
    filesOnly: true,
    maxFiles: LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT,
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
  }
  const r = await searchContentRipgrep(tq as LocalSearchToolQuery);
  prov.push({ backend: 'localSearchCode', source: query.from });
  return rowsByPath(
    (r.files ?? []).map(file => ({
      kind: 'file' as const,
      source,
      path: file.path,
      entryType: 'file' as const,
    }))
  );
}

export async function executeFilesWithoutMatch(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  where: Predicate & { kind: 'not'; predicate: Predicate }
): Promise<AdapterResult> {
  const result = await searchContentRipgrep(
    withoutMatchToolQuery(
      query,
      searchPath,
      where.predicate
    ) as LocalSearchToolQuery
  );
  const diagnostics = resultDiagnostics(result, 'localSearchCode');
  const files = (result.files ?? []).map(r => ({
    kind: 'file' as const,
    source,
    path: r.path,
    entryType: 'file' as const,
  }));
  return {
    results: files,
    diagnostics,
    provenance: [provenance('localSearchCode', source, where)],
  };
}

export async function executeCodeFilesWithoutMatch(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  where: Predicate & { kind: 'not'; predicate: Predicate }
): Promise<AdapterResult> {
  const result = await searchContentRipgrep(
    withoutMatchToolQuery(
      query,
      searchPath,
      where.predicate
    ) as LocalSearchToolQuery
  );
  const mapped = mapCodeResult(result, source);
  return {
    ...mapped,
    diagnostics: resultDiagnostics(result, 'localSearchCode'),
    provenance: [provenance('localSearchCode', source, where)],
  };
}

function withoutMatchToolQuery(
  query: OqlQuery,
  searchPath: string,
  leaf: Predicate
): Partial<LocalSearchToolQuery> {
  const compiled = compileWhere(leaf);
  const m = compiled.match!;
  const toolQuery: Partial<LocalSearchToolQuery> = {
    path: searchPath,
    filesWithoutMatch: true,
    ...scopeToCommon(query.scope),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
    ...searchControls(query),
  };
  applyLocalSearchLanguage(toolQuery, query.scope, m.langType);

  toolQuery.keywords = m.keywords;
  if (m.fixedString) toolQuery.fixedString = true;
  if (m.perlRegex) toolQuery.perlRegex = true;
  if (m.caseSensitive) toolQuery.caseSensitive = true;
  if (m.caseInsensitive) toolQuery.caseInsensitive = true;
  if (m.wholeWord) toolQuery.wholeWord = true;
  if (m.multiline) toolQuery.multiline = true;
  if (m.multilineDotall) toolQuery.multilineDotall = true;
  return toolQuery;
}
