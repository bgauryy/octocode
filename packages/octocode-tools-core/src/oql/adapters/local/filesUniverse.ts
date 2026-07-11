/**
 * The file-attribute (field-predicate) side of the files-target set algebra:
 * the candidate universe and single field-leaf lookups, both backed by the
 * local file-finder runner.
 */
import { findFiles } from '../../../tools/local_find_files/findFiles.js';
import { mapFilesResult } from '../resultMap.js';
import {
  applyFindFilesScope,
  applyUnpagedFindFilesWindow,
  findFilesNeedsScopePostFilter,
} from './scope.js';
import { applyFieldPredicate } from './predicates.js';
import { filterFindFilesResultByScope } from './structureContent.js';
import { rowsByPath } from './fileRowOps.js';
import type {
  OqlDiagnostic,
  OqlFileResultRow,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QuerySource,
} from '../../types.js';
import type { FileRowMap } from './types.js';
import type { LocalFindToolQuery } from './findToolType.js';

/** All files in scope — the candidate universe for negation. */
export async function universeFileRows(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[]
): Promise<FileRowMap> {
  const toolQuery: Partial<LocalFindToolQuery> = {
    path: searchPath,
    entryType: 'f',
  };
  applyFindFilesScope(toolQuery, query.scope);
  if (findFilesNeedsScopePostFilter(query.scope)) {
    applyUnpagedFindFilesWindow(toolQuery);
    toolQuery.page = 1;
  }
  const result = await findFiles(toolQuery as LocalFindToolQuery);
  const scopedResult = filterFindFilesResultByScope(result, query, searchPath);
  prov.push({ backend: 'localFindFiles', source: query.from });
  // content-predicate negation applies to files, not directory entries
  return rowsByPath(
    mapFilesResult(
      {
        ...scopedResult,
        files: (scopedResult.files ?? []).filter(
          f => f.type === undefined || f.type === 'f' || f.type === 'file'
        ),
      },
      source
    ).results as OqlFileResultRow[]
  );
}

/** File-set for a single field-predicate leaf (attribute lookup). */
export async function leafFieldFileRows(
  query: OqlQuery,
  leaf: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<FileRowMap> {
  const tq: Partial<LocalFindToolQuery> = { path: searchPath, details: true };
  applyFindFilesScope(tq, query.scope);
  applyFieldPredicate(leaf, tq, diags);
  if (findFilesNeedsScopePostFilter(query.scope)) {
    applyUnpagedFindFilesWindow(tq);
    tq.page = 1;
  }
  const r = await findFiles(tq as LocalFindToolQuery);
  const scopedResult = filterFindFilesResultByScope(r, query, searchPath);
  prov.push({ backend: 'localFindFiles', source: query.from });
  return rowsByPath(
    mapFilesResult(scopedResult, source).results as OqlFileResultRow[]
  );
}
