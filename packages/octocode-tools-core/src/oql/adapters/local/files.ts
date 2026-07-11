/**
 * `target:"files"` execution: field predicates map to `localFindFiles`
 * attributes, content predicates re-shape a `localSearchCode` (discovery)
 * run into file rows, and boolean/negated predicates delegate to the
 * set-algebra helpers in filesBoolean.ts.
 */
import { findFiles } from '../../../tools/local_find_files/findFiles.js';
import { mapFilesResult } from '../resultMap.js';
import {
  applyFindFilesPagination,
  applyFindFilesScope,
  applyFindFilesSort,
} from './scope.js';
import {
  isBooleanPredicate,
  isContentPredicate,
  isNegatedRipgrepContentLeaf,
  applyFieldPredicate,
  resultDiagnostics,
  provenance,
} from './predicates.js';
import { executeCode } from './code.js';
import { executeFilesBoolean } from './filesBoolean.js';
import { executeFilesWithoutMatch } from './filesContentLeaf.js';
import { filterFindFilesResultByScope } from './structureContent.js';
import type { OqlDiagnostic, OqlQuery, QuerySource } from '../../types.js';
import type { AdapterResult } from './types.js';
import type { LocalFindToolQuery } from './findToolType.js';

export async function executeFiles(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const where = query.where;

  if (isNegatedRipgrepContentLeaf(where)) {
    return executeFilesWithoutMatch(query, source, searchPath, where);
  }

  // Boolean composition (all/any/not) is well-defined at the FILE level:
  // all=intersection, any=union, not=universe−set. Local scope IS the complete
  // candidate universe, so negation is exact (no negativeUniverseRequired).
  if (where && isBooleanPredicate(where)) {
    return executeFilesBoolean(query, source, searchPath, where);
  }

  // A content predicate on `files` means "file contains a match" -> ripgrep
  // filesOnly. A field predicate maps to findFiles attributes.
  if (where && isContentPredicate(where)) {
    const codeResult = await executeCode(
      { ...query, view: 'discovery' },
      source,
      searchPath
    );
    // Re-shape code rows (one per file) into file rows.
    const files = codeResult.results.map(r => ({
      kind: 'file' as const,
      source,
      path: (r as { path: string }).path,
      entryType: 'file' as const,
    }));
    return { ...codeResult, results: files };
  }

  const toolQuery: Partial<LocalFindToolQuery> = {
    path: searchPath,
    details: true,
    showFileLastModified: true,
  };
  applyFindFilesScope(toolQuery, query.scope);
  applyFindFilesPagination(toolQuery, query);
  applyFindFilesSort(toolQuery, query);

  const fieldDiags: OqlDiagnostic[] = [];
  if (where) {
    applyFieldPredicate(where, toolQuery, fieldDiags);
  }
  const result = await findFiles(toolQuery as LocalFindToolQuery);
  const scopedResult = filterFindFilesResultByScope(result, query, searchPath);
  const mapped = mapFilesResult(scopedResult, source);
  return {
    ...mapped,
    diagnostics: [
      ...fieldDiags,
      ...resultDiagnostics(scopedResult, 'localFindFiles'),
    ],
    provenance: [provenance('localFindFiles', source, where)],
  };
}
