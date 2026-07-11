/**
 * `target:"code"` execution: compile a (possibly boolean) `where` predicate
 * into `localSearchCode` calls and map results into OQL code rows, including
 * the self-correcting diagnostics that guard against false-absence traps.
 */
import { searchContentRipgrep } from '../../../tools/local_ripgrep/searchContentRipgrep.js';
import { compileWhere } from '../compile.js';
import { mapCodeResult } from '../resultMap.js';
import { diagnostic } from '../../diagnostics.js';
import {
  scopeToCommon,
  applyLocalSearchLanguage,
  looksLikeRegex,
  searchControls,
} from './scope.js';
import {
  needsBooleanEval,
  isNegatedRipgrepContentLeaf,
  resultDiagnostics,
  provenance,
} from './predicates.js';
import { executeCodeBoolean } from './codeBoolean.js';
import { executeCodeFilesWithoutMatch } from './filesContentLeaf.js';
import type { OqlQuery, Predicate, QuerySource } from '../../types.js';
import type { AdapterResult } from './types.js';
import type { LocalSearchToolQuery } from './searchToolType.js';

export async function executeCode(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const where = query.where as Predicate;

  // Boolean predicate over code: evaluate each leaf and combine match rows by
  // file-set algebra (all = intersection, any = union, not = universe−set).
  // localSearchCode is single-pattern, so a multi-leaf boolean cannot be one
  // call — but it IS expressible as set logic over per-leaf match rows.
  if (needsBooleanEval(where)) {
    return executeCodeBoolean(query, source, searchPath, where);
  }

  if (query.view === 'discovery' && isNegatedRipgrepContentLeaf(where)) {
    return executeCodeFilesWithoutMatch(query, source, searchPath, where);
  }

  const compiled = compileWhere(where);
  if (compiled.unsupported) {
    return {
      results: [],
      diagnostics: [
        diagnostic(compiled.unsupported.code, compiled.unsupported.message, {
          backend: 'localSearchCode',
          ...(compiled.unsupported.predicateId
            ? { predicateId: compiled.unsupported.predicateId }
            : {}),
        }),
      ],
      provenance: [],
    };
  }

  const m = compiled.match!;
  const toolQuery: Partial<LocalSearchToolQuery> = {
    path: searchPath,
    ...scopeToCommon(query.scope),
    ...(query.view === 'discovery' ? { filesOnly: true } : {}),
    ...(query.view === 'detailed' ? { contextLines: 3 } : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
    ...searchControls(query),
  };
  applyLocalSearchLanguage(toolQuery, query.scope, m.langType);

  if (m.mode === 'structural') {
    toolQuery.mode = 'structural';
    if (m.pattern !== undefined) toolQuery.pattern = m.pattern;
    if (typeof m.rule === 'string') toolQuery.rule = m.rule;
  } else {
    toolQuery.keywords = m.keywords;
    if (m.fixedString) toolQuery.fixedString = true;
    if (m.perlRegex) toolQuery.perlRegex = true;
    if (m.caseSensitive) toolQuery.caseSensitive = true;
    if (m.caseInsensitive) toolQuery.caseInsensitive = true;
    if (m.wholeWord) toolQuery.wholeWord = true;
    if (m.multiline) toolQuery.multiline = true;
    if (m.multilineDotall) toolQuery.multilineDotall = true;
    if (compiled.negate) toolQuery.invertMatch = true;
  }

  const result = await searchContentRipgrep(toolQuery as LocalSearchToolQuery);
  const mapped = mapCodeResult(result, source);
  const diagnostics = resultDiagnostics(result, 'localSearchCode');

  // Per-file match cap: localSearchCode returns up to maxMatchesPerFile (default
  // ~10) matches per file. If a file has more, that's a pagination boundary, not
  // a failure — emit a NON-blocking info note. The result stays evidence:partial
  // (driven by the next.matchPage open-page signal, like next.page), so it never
  // claims false proof of completeness, but it isn't framed as "truncated/blocked".
  const truncatedFiles = matchTruncatedFiles(result);
  if (truncatedFiles.length > 0) {
    const total = truncatedFiles.reduce((n, f) => n + (f.total ?? 0), 0);
    const shown = truncatedFiles.reduce((n, f) => n + f.shown, 0);
    diagnostics.push(
      diagnostic(
        'matchTruncated',
        `${truncatedFiles.length} file(s) have more matches (showed ${shown} of ${total}) — page with controls.search.matchPage, or raise controls.search.maxMatchesPerFile.`,
        {
          backend: 'localSearchCode',
          severity: 'info',
          blocksAnswer: false,
          repair: {
            message:
              'Follow next.matchPage to page within files, or set controls.search.maxMatchesPerFile higher.',
          },
        }
      )
    );
  }

  // Self-correcting structural empty: a `pattern` (not a rule) that matched 0
  // nodes is the #1 agent failure — almost always "pattern too specific" (the
  // real node has a return type / typed params the pattern omitted), not genuine
  // absence. Hand back the robust fix (a rule for named lookup) so the next call
  // succeeds. Non-blocking: 0 matches is still a valid result, this is guidance.
  if (
    m.mode === 'structural' &&
    m.pattern !== undefined &&
    mapped.results.length === 0
  ) {
    // The concrete `): $R {` return-type suggestion is already emitted once by
    // the backing tool (structuralSearch.ts → partialResult), which flows
    // through on this path; don't repeat it here. The unique value this
    // adapter adds is the rule-based lookup shape in `repair` below.
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        'Structural pattern matched 0 nodes. A pattern must match the COMPLETE node — if the target has a return type or typed params the pattern omits, it returns 0 (not genuine absence).',
        {
          backend: 'localSearchCode',
          severity: 'info',
          blocksAnswer: false,
          repair: {
            message:
              'To find a named symbol, prefer a rule over a pattern: where = { kind:"structural", lang, rule:{ kind:"<node e.g. function_declaration>", has:{ pattern:"<name>" } } }. Or complete the pattern (e.g. add a return type `: $R`).',
          },
        }
      )
    );
  }

  // Literal-search false-absence guard: a `text` predicate is searched as a
  // fixed string, so regex metacharacters in the value match literally. A
  // 0-result literal search whose value looks like a regex (e.g. "a|b",
  // "\bfoo\b") is the classic false-absence trap — nudge toward a regex
  // predicate rather than letting the caller conclude the term is absent.
  if (
    m.mode !== 'structural' &&
    m.fixedString === true &&
    mapped.results.length === 0 &&
    m.keywords !== undefined &&
    looksLikeRegex(m.keywords)
  ) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `Literal search for "${m.keywords}" matched 0 — a text predicate searches for a FIXED string, so the regex metacharacters in it were matched literally (not as a pattern). This may be a false "not found".`,
        {
          backend: 'localSearchCode',
          severity: 'info',
          blocksAnswer: false,
          repair: {
            message:
              'If you meant a pattern, use a regex predicate (where = { kind:"regex", value:"<pattern>" }) so metacharacters like | ( ) [ ] ^ $ * + ? \\b are interpreted.',
          },
        }
      )
    );
  }

  return {
    ...mapped,
    diagnostics,
    provenance: [provenance('localSearchCode', source, where)],
  };
}

/** Files whose returned matches are fewer than their true match count. */
function matchTruncatedFiles(result: {
  files?: Array<{
    path: string;
    matchCount?: number;
    totalMatchRows?: number;
    returnedMatchRows?: number;
    matches?: unknown[];
    pagination?: { hasMore?: boolean; totalMatches?: number };
  }>;
}): Array<{ path: string; shown: number; total?: number }> {
  const out: Array<{ path: string; shown: number; total?: number }> = [];
  for (const f of result.files ?? []) {
    const shown = f.returnedMatchRows ?? f.matches?.length ?? 0;
    const total =
      f.pagination?.totalMatches ?? f.totalMatchRows ?? f.matchCount;
    const moreByFlag = f.pagination?.hasMore === true;
    const moreByCount = typeof total === 'number' && total > shown;
    if (moreByFlag || moreByCount) {
      out.push({ path: f.path, shown, total });
    }
  }
  return out;
}
