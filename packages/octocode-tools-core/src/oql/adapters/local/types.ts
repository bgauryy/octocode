/**
 * Shared types for the local execution adapter split: the public
 * `AdapterResult` shape, tool-query type aliases derived from each local
 * runner's real input contract, and small internal helper types used across
 * the sibling modules in this directory.
 *
 * `LocalSearchToolQuery` and `LocalFindToolQuery` live in their own
 * single-purpose sibling files (searchToolType.ts / findToolType.ts) rather
 * than here, so that no module other than those two needs a runtime import
 * of both backing runners at once.
 */
import { viewStructure } from '../../../tools/local_view_structure/local_view_structure.js';
import { fetchContent } from '../../../tools/local_fetch_content/fetchContent.js';
import type { MappedResult } from '../resultMap.js';
import type {
  OqlCodeResultRow,
  OqlDiagnostic,
  OqlFileResultRow,
  OqlProvenance,
} from '../../types.js';

export interface AdapterResult extends MappedResult {
  shared?: Record<string, unknown>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
}

// Type the compiled tool queries against each runner's real input contract
// instead of `Record<string, unknown>`, so field construction is checked.
// viewStructure reads `page`/`itemsPerPage` at runtime (see its executor)
// but omits them from its declared input type — name them here rather than
// hiding the gap behind a cast. Builders type as `Partial<…>` because schema
// defaults (mode/sort/matchContentLength/contextLines/minify) are filled by
// the tool's own validation, not by the adapter.
export type LocalStructureToolQuery = Parameters<typeof viewStructure>[0] & {
  page?: number;
  itemsPerPage?: number;
};
export type LocalFetchToolQuery = Parameters<typeof fetchContent>[0];

export type FileRowMap = Map<string, OqlFileResultRow>;

/** A boolean-code-eval partial result: satisfying files + positive rows. */
export interface CodeEval {
  /** Files satisfying this sub-predicate (the candidate universe slice). */
  files: Set<string>;
  /** Positive match occurrences (empty for negation/field constraints). */
  rows: OqlCodeResultRow[];
}

// Shared glob-path normalization used both when classifying scope include
// globs (scope.ts) and when matching real file paths against scope globs
// (structureContent.ts) — kept here so neither module depends on the other.
export function normalizeGlobPath(value: string): string {
  return value.replace(/\\/g, '/').replace(/^\.\//, '');
}
