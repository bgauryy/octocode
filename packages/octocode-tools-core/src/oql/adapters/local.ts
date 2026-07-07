/**
 * Local execution adapter: compile a canonical OQL query into the existing
 * local tool runners (`searchContentRipgrep`, `findFiles`, `viewStructure`,
 * `fetchContent`) and map their typed results into OQL rows.
 *
 * These lower-level runners perform path validation internally, so the
 * security contract (path bounds) survives this adapter. Secret sanitization
 * is applied by the interface layer on final output.
 */
import path from 'node:path';
import fs from 'node:fs';
import type { LocalFindFilesToolResult } from '@octocodeai/octocode-core/extra-types';
import { LOCAL_MAX_LIMIT } from '../../config.js';
import { searchContentRipgrep } from '../../tools/local_ripgrep/searchContentRipgrep.js';
import { findFiles } from '../../tools/local_find_files/findFiles.js';
import { viewStructure } from '../../tools/local_view_structure/local_view_structure.js';
import { fetchContent } from '../../tools/local_fetch_content/fetchContent.js';
import { compileWhere } from './compile.js';
import {
  mapCodeResult,
  mapContentResult,
  mapFilesResult,
  mapStructureResult,
  type MappedResult,
} from './resultMap.js';
import { diagnostic } from '../diagnostics.js';
import {
  toLocalFileLanguageGlobs,
  toLocalSearchLanguageParams,
} from '../transformers/language.js';
import {
  firstScopePath,
  firstScopeLanguage,
} from '../transformers/github/common.js';
import type {
  FieldPredicate,
  OqlDiagnostic,
  OqlFileResultRow,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QueryScope,
  QuerySource,
} from '../types.js';

export interface AdapterResult extends MappedResult {
  shared?: Record<string, unknown>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
}

// Type the compiled tool queries against each runner's real input contract
// instead of `Record<string, unknown>`, so field construction is checked.
// findFiles/viewStructure read `page`/`itemsPerPage` at runtime (see their
// executors) but omit them from their declared input types — name them here
// rather than hiding the gap behind a cast. Builders type as `Partial<…>`
// because schema defaults (mode/sort/matchContentLength/contextLines/minify)
// are filled by the tool's own validation, not by the adapter.
type LocalSearchToolQuery = Parameters<typeof searchContentRipgrep>[0];
type LocalFindToolQuery = Parameters<typeof findFiles>[0] & {
  page?: number;
  itemsPerPage?: number;
};
type LocalStructureToolQuery = Parameters<typeof viewStructure>[0] & {
  page?: number;
  itemsPerPage?: number;
};
type LocalFetchToolQuery = Parameters<typeof fetchContent>[0];

const LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT = LOCAL_MAX_LIMIT;

type FileRowMap = Map<string, OqlFileResultRow>;

/** Resolve the local filesystem root for a query (local or materialized). */
function localRoot(query: OqlQuery): string {
  if (query.from?.kind === 'local') return query.from.path;
  if (query.from?.kind === 'materialized') return query.from.localPath;
  throw new Error('localExecute requires a local or materialized source.');
}

function scopeToCommon(
  scope: QueryScope | undefined
): Partial<LocalSearchToolQuery> {
  const out: Partial<LocalSearchToolQuery> = {};
  if (scope?.include) out.include = scope.include;
  if (scope?.exclude) out.exclude = scope.exclude;
  if (scope?.excludeDir) out.excludeDir = scope.excludeDir;
  if (scope?.hidden !== undefined) out.hidden = scope.hidden;
  if (scope?.noIgnore !== undefined) out.noIgnore = scope.noIgnore;
  return out;
}

// Signals that a literal (fixedString) search value was probably meant as a
// regex: alternation, groups, character classes, anchors, quantifiers, or a
// backslash class escape. Lone `.` is excluded — it is far too common in real
// literal searches (paths, method calls) to treat as a regex intent.
const REGEX_METACHAR_RE = /[|()[\]{}^$*+?]|\\[bBdDwWsS]/;

function looksLikeRegex(value: string): boolean {
  return REGEX_METACHAR_RE.test(value);
}

function mergeStringArrays(left: unknown, right: readonly string[]): string[] {
  const existing = Array.isArray(left)
    ? left.filter((value): value is string => typeof value === 'string')
    : [];
  return [...new Set([...existing, ...right])];
}

function applyLocalSearchLanguage(
  toolQuery: Partial<LocalSearchToolQuery>,
  scope: QueryScope | undefined,
  explicitLangType: string | undefined
): void {
  if (explicitLangType) {
    toolQuery.langType = explicitLangType;
    return;
  }

  const languageParams = toLocalSearchLanguageParams(firstScopeLanguage(scope));
  if (languageParams.langType) toolQuery.langType = languageParams.langType;
  if (languageParams.include?.length) {
    toolQuery.include = mergeStringArrays(
      toolQuery.include,
      languageParams.include
    );
  }
}

function languageNameGlobs(scope: QueryScope | undefined): string[] {
  return toLocalFileLanguageGlobs(firstScopeLanguage(scope));
}

function scopeIncludeAsName(glob: string): string | undefined {
  const normalized = normalizeGlobPath(glob);
  const recursiveExtension = normalized.match(/^\*\*\/(\*\.[^/]+)$/);
  if (recursiveExtension) return recursiveExtension[1];
  if (!normalized.includes('/')) return normalized;
  return undefined;
}

function findFilesNeedsScopePostFilter(scope: QueryScope | undefined): boolean {
  return Boolean(scope?.include?.length || scope?.exclude?.length);
}

function applyFindFilesScope(
  toolQuery: Partial<LocalFindToolQuery>,
  scope: QueryScope | undefined
): void {
  if (scope?.excludeDir) toolQuery.excludeDir = scope.excludeDir;
  if (scope?.minDepth !== undefined) toolQuery.minDepth = scope.minDepth;
  if (scope?.maxDepth !== undefined) toolQuery.maxDepth = scope.maxDepth;

  const includeNames = (scope?.include ?? [])
    .map(scopeIncludeAsName)
    .filter((value): value is string => Boolean(value));
  if (includeNames.length > 0 && !toolQuery.names) {
    toolQuery.names = mergeStringArrays(toolQuery.names, includeNames);
  }

  const pathIncludes = (scope?.include ?? []).filter(
    glob => !scopeIncludeAsName(glob)
  );
  if (pathIncludes.length === 1 && !toolQuery.pathPattern) {
    toolQuery.pathPattern = pathIncludes[0];
  }

  const languageNames = languageNameGlobs(scope);
  if (languageNames.length > 0 && !toolQuery.names && !toolQuery.pathPattern) {
    toolQuery.names = languageNames;
  }
}

function applyFindFilesPagination(
  toolQuery: Partial<LocalFindToolQuery>,
  query: OqlQuery
): void {
  if (findFilesNeedsScopePostFilter(query.scope)) {
    applyUnpagedFindFilesWindow(toolQuery);
    toolQuery.page = 1;
    return;
  }
  if (query.itemsPerPage) toolQuery.itemsPerPage = query.itemsPerPage;
  if (query.page) toolQuery.page = query.page;
}

function applyUnpagedFindFilesWindow(
  toolQuery: Partial<LocalFindToolQuery>
): void {
  toolQuery.limit = LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT;
  toolQuery.itemsPerPage = LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT;
}

export async function executeLocal(query: OqlQuery): Promise<AdapterResult> {
  // dispatch only routes local/materialized code/content/structure/files here.
  const source = query.from as QuerySource;
  const root = localRoot(query);
  const scopePath = firstScopePath(query.scope);
  const searchPath = scopePath ? path.join(root, scopePath) : root;

  // Path-existence guard: ripgrep/find/structure on a non-existent path return
  // zero rows, which the adapter would otherwise map to a clean `zeroMatches`
  // with evidence:proof / answerReady:true — falsely confirming absence when
  // the path was simply a typo. Surface a blocking `invalidQuery` error instead
  // so an agent corrects the path rather than concluding "not found".
  if (!fs.existsSync(searchPath)) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          `Local path does not exist: ${searchPath}. Check the path/spelling (and branch or materialization for remote sources) before treating this as absence.`,
          {
            backend: 'localExecute',
            queryPath: searchPath,
            repair: {
              message:
                'Verify the path exists (orient with target:"structure" on a known-good parent), fix typos, or materialize the remote source first.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  switch (query.target) {
    case 'files':
      return executeFiles(query, source, searchPath);
    case 'structure':
      return executeStructure(query, source, searchPath);
    case 'content':
      return executeContent(query, source, searchPath);
    case 'code':
    default:
      return executeCode(query, source, searchPath);
  }
}

/** A boolean `where` that needs multi-call evaluation (not a single leaf). */
function needsBooleanEval(p: Predicate): boolean {
  if (p.kind === 'all' || p.kind === 'any') return true;
  // not(leaf) compiles to a single call; not(boolean) needs set algebra.
  if (p.kind === 'not') return isBooleanPredicate(p.predicate);
  return false;
}

function isRipgrepContentLeaf(p: Predicate): boolean {
  return p.kind === 'text' || p.kind === 'regex';
}

function isNegatedRipgrepContentLeaf(
  p: Predicate | undefined
): p is Predicate & { kind: 'not'; predicate: Predicate } {
  return p?.kind === 'not' && isRipgrepContentLeaf(p.predicate);
}

async function executeCode(
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

async function executeFiles(
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

// localFindFiles orders by sortBy modified|name|path|size; other
// controls.search.sort values are code-search sorts with no files-lane
// equivalent and are left to the default ordering.
function applyFindFilesSort(
  toolQuery: Partial<LocalFindToolQuery>,
  query: OqlQuery
): void {
  const sort = query.controls?.search?.sort;
  if (
    sort === 'size' ||
    sort === 'name' ||
    sort === 'path' ||
    sort === 'modified'
  ) {
    toolQuery.sortBy = sort;
  }
}

function isBooleanPredicate(p: Predicate): boolean {
  return p.kind === 'all' || p.kind === 'any' || p.kind === 'not';
}

/**
 * Evaluate a boolean `where` over the FILES target with file-level set algebra.
 * Each leaf runs as its own backend call (localSearchCode filesOnly for content
 * predicates, localFindFiles for field predicates); results combine as sets.
 */
async function executeFilesBoolean(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string,
  where: Predicate
): Promise<AdapterResult> {
  const provenance: OqlProvenance[] = [];
  const diagnostics: OqlDiagnostic[] = [];
  const set = await evalFilesPredicate(
    query,
    where,
    source,
    searchPath,
    provenance,
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
  return { results: rows, diagnostics, provenance };
}

// Order boolean-combined file rows like localFindFiles would: size and
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

async function executeFilesWithoutMatch(
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

async function executeCodeFilesWithoutMatch(
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

function intersectFileRows(a: FileRowMap, b: FileRowMap): FileRowMap {
  const out: FileRowMap = new Map();
  for (const [path, row] of a) {
    const other = b.get(path);
    if (other) out.set(path, mergeFileRows(row, other));
  }
  return out;
}

function intersectStringSets(a: Set<string>, b: Set<string>): Set<string> {
  return new Set([...a].filter(value => b.has(value)));
}

function mergeFileRows(
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

/** All files in scope — the candidate universe for negation. */
async function universeFiles(
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

async function universeFileRows(
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

/** File-set for a single (non-boolean) predicate. */
async function leafFiles(
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

async function leafFileRows(
  query: OqlQuery,
  leaf: Predicate,
  source: QuerySource,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<FileRowMap> {
  if (leaf.kind === 'field') {
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
  // content leaf (text/regex/structural) -> filesOnly search
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

function rowsByPath(rows: OqlFileResultRow[]): FileRowMap {
  const out: FileRowMap = new Map();
  for (const row of rows) {
    out.set(row.path, mergeFileRows(out.get(row.path), row));
  }
  return out;
}

/* ---------------------- boolean evaluation over code -------------------- */

interface CodeEval {
  /** Files satisfying this sub-predicate (the candidate universe slice). */
  files: Set<string>;
  /** Positive match occurrences (empty for negation/field constraints). */
  rows: import('../types.js').OqlCodeResultRow[];
}

/**
 * Evaluate a boolean `where` over `target:"code"` and return match rows.
 * Match rows come only from positive (non-negated) content leaves; `not` and
 * `field` leaves contribute file-set constraints, not occurrences. Local scope
 * is the complete universe, so negation is exact.
 */
async function executeCodeBoolean(
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
): Promise<import('../types.js').OqlCodeResultRow[]> {
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
  return mapCodeResult(r, source)
    .results as import('../types.js').OqlCodeResultRow[];
}

async function executeStructure(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const toolQuery: Partial<LocalStructureToolQuery> = {
    path: searchPath,
    // details:true makes the tool emit per-entry rows (with depth/size) rather
    // than grouped files/folders lists, which map cleanly to tree rows.
    details: true,
    ...(query.fetch?.tree?.maxDepth !== undefined
      ? { maxDepth: query.fetch.tree.maxDepth, recursive: true }
      : {}),
    ...(query.fetch?.tree?.pattern
      ? { pattern: query.fetch.tree.pattern }
      : {}),
    ...(query.fetch?.tree?.includeSizes ? { includeSizes: true } : {}),
    ...(query.fetch?.tree?.extensions?.length
      ? { extensions: query.fetch.tree.extensions }
      : {}),
    ...(query.fetch?.tree?.filesOnly ? { filesOnly: true } : {}),
    ...(query.fetch?.tree?.directoriesOnly ? { directoriesOnly: true } : {}),
    ...(query.fetch?.tree?.sortBy ? { sortBy: query.fetch.tree.sortBy } : {}),
    ...(query.fetch?.tree?.reverse ? { reverse: true } : {}),
    ...(query.scope?.hidden !== undefined
      ? { hidden: query.scope.hidden }
      : {}),
    ...(query.limit ? { limit: query.limit } : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
  };
  const result = await viewStructure(toolQuery as LocalStructureToolQuery);
  const mapped = mapStructureResult(result, source);
  return {
    ...mapped,
    diagnostics: resultDiagnostics(result, 'localViewStructure'),
    provenance: [provenance('localViewStructure', source, undefined)],
  };
}

async function executeContent(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const c = query.fetch?.content;
  const minify =
    c?.contentView === 'exact'
      ? 'none'
      : c?.contentView === 'symbols'
        ? 'symbols'
        : 'standard';
  const range = normalizeContentRange(c?.range);
  const toolQuery: Partial<LocalFetchToolQuery> = {
    path: searchPath,
    minify,
    ...range,
    ...(c?.match?.text !== undefined ? { matchString: c.match.text } : {}),
    ...(c?.match?.regex ? { matchStringIsRegex: true } : {}),
    ...(c?.match?.caseSensitive ? { matchStringCaseSensitive: true } : {}),
    // Forward contextLines when startLine is absent (match-only fetch) so the
    // tool can expand context around the matched lines natively.
    ...(c?.range?.contextLines !== undefined &&
    c?.range?.startLine === undefined
      ? { contextLines: c.range.contextLines }
      : {}),
    ...(c?.charOffset !== undefined ? { charOffset: c.charOffset } : {}),
    ...(c?.charLength !== undefined ? { charLength: c.charLength } : {}),
    ...(c?.fullContent ? { fullContent: true } : {}),
  };
  const result = await fetchContent(toolQuery as LocalFetchToolQuery);
  const requestedView =
    c?.contentView === 'exact'
      ? 'exact'
      : c?.contentView === 'symbols'
        ? 'symbols'
        : 'compact';
  const mapped = mapContentResult(result, source, searchPath, requestedView);
  return {
    ...mapped,
    diagnostics: resultDiagnostics(result, 'localGetFileContent'),
    provenance: [provenance('localGetFileContent', source, undefined)],
  };
}

/* ------------------------------ helpers --------------------------------- */

function filterFindFilesResultByScope(
  result: LocalFindFilesToolResult,
  query: OqlQuery,
  searchPath: string
): LocalFindFilesToolResult {
  if (!findFilesNeedsScopePostFilter(query.scope)) return result;
  if ((result as { status?: string }).status === 'error') return result;

  const filtered = (result.files ?? []).filter(entry =>
    pathMatchesScope(entry.path, searchPath, query.scope)
  );
  const page = Math.max(1, query.page ?? 1);
  const itemsPerPage = Math.max(
    1,
    (query.itemsPerPage ?? query.limit ?? filtered.length) || 1
  );
  const totalPages = Math.max(1, Math.ceil(filtered.length / itemsPerPage));
  const start = (page - 1) * itemsPerPage;
  const files = filtered.slice(start, start + itemsPerPage);
  const { status: _status, ...rest } = result as LocalFindFilesToolResult & {
    status?: string;
  };

  return {
    ...rest,
    ...(files.length === 0 ? { status: 'empty' as const } : {}),
    files,
    pagination: {
      currentPage: page,
      totalPages,
      filesPerPage: itemsPerPage,
      totalFiles: filtered.length,
      hasMore: page < totalPages,
      ...(page < totalPages ? { nextPage: page + 1 } : {}),
    },
  };
}

function pathMatchesScope(
  filePath: string,
  searchPath: string,
  scope: QueryScope | undefined
): boolean {
  const relativePath = normalizeGlobPath(
    relativeOrAbsolutePath(filePath, searchPath)
  );
  const includes = scope?.include ?? [];
  if (
    includes.length > 0 &&
    !includes.some(glob => matchesGlob(relativePath, glob))
  ) {
    return false;
  }
  return !(scope?.exclude ?? []).some(glob => matchesGlob(relativePath, glob));
}

function relativeOrAbsolutePath(filePath: string, searchPath: string): string {
  const rel = path.relative(searchPath, filePath);
  if (!rel || rel.startsWith('..') || path.isAbsolute(rel)) return filePath;
  return rel;
}

function normalizeGlobPath(value: string): string {
  return value.replace(/\\/g, '/').replace(/^\.\//, '');
}

function matchesGlob(relativePath: string, glob: string): boolean {
  const normalizedGlob = normalizeGlobPath(glob);
  const target = normalizeGlobPath(relativePath);
  const re = globToRegExp(normalizedGlob);
  if (re.test(target)) return true;
  if (!normalizedGlob.includes('/')) {
    return re.test(path.posix.basename(target));
  }
  return false;
}

function globToRegExp(glob: string): RegExp {
  let pattern = '^';
  for (let i = 0; i < glob.length; i++) {
    const ch = glob[i]!;
    const next = glob[i + 1];
    const afterNext = glob[i + 2];
    if (ch === '*' && next === '*' && afterNext === '/') {
      pattern += '(?:.*/)?';
      i += 2;
      continue;
    }
    if (ch === '*' && next === '*') {
      pattern += '.*';
      i += 1;
      continue;
    }
    if (ch === '*') {
      pattern += '[^/]*';
      continue;
    }
    if (ch === '?') {
      pattern += '[^/]';
      continue;
    }
    pattern += escapeRegExp(ch);
  }
  return new RegExp(`${pattern}$`);
}

function escapeRegExp(ch: string): string {
  return /[|\\{}()[\]^$+?.]/.test(ch) ? `\\${ch}` : ch;
}

function normalizeContentRange(
  range: NonNullable<NonNullable<OqlQuery['fetch']>['content']>['range']
): { startLine?: number; endLine?: number } {
  if (range?.startLine === undefined) return {};
  const contextLines = range.contextLines ?? 0;
  const startLine = Math.max(1, range.startLine - contextLines);
  const endLine = (range.endLine ?? range.startLine) + contextLines;
  return { startLine, endLine };
}

function isContentPredicate(p: Predicate): boolean {
  if (p.kind === 'text' || p.kind === 'regex' || p.kind === 'structural') {
    return true;
  }
  if (p.kind === 'not') return isContentPredicate(p.predicate);
  return false;
}

function applyFieldPredicate(
  where: Predicate,
  toolQuery: Partial<LocalFindToolQuery>,
  diags: OqlDiagnostic[]
): void {
  const negate = where.kind === 'not';
  const inner = where.kind === 'not' ? where.predicate : where;
  if (inner.kind !== 'field') {
    diags.push(
      diagnostic(
        'unsupportedPredicate',
        'Only field predicates (and field-negation) compile to the files backend.',
        { backend: 'localFindFiles' }
      )
    );
    return;
  }
  const f = inner as FieldPredicate;
  const value = f.value;
  switch (f.field) {
    case 'basename':
    case 'path':
      if (f.op === 'regex') toolQuery.regex = String(value);
      else if (f.op === 'glob' || f.op === '=' || f.op === 'in') {
        toolQuery.names = Array.isArray(value)
          ? value.map(String)
          : [String(value)];
      } else {
        diags.push(unsupportedField(f));
      }
      break;
    case 'extension': {
      const exts = (Array.isArray(value) ? value : [value]).map(
        v => `*.${String(v).replace(/^\./, '')}`
      );
      if (f.op === '=' || f.op === 'in' || f.op === 'glob')
        toolQuery.names = exts;
      else diags.push(unsupportedField(f));
      break;
    }
    case 'size':
      if (f.op === '>' || f.op === '>=') toolQuery.sizeGreater = String(value);
      else if (f.op === '<' || f.op === '<=')
        toolQuery.sizeLess = String(value);
      else diags.push(unsupportedField(f));
      break;
    case 'modified':
      // findFiles only has RELATIVE windows (modifiedWithin/Before take "7d").
      // It has no absolute-date filter, so >/>=/</<= (absolute timestamps)
      // are unsupported — mapping them to a duration field would be both a
      // type mismatch and a semantic inversion.
      if (f.op === 'within') toolQuery.modifiedWithin = String(value);
      else if (f.op === 'before') toolQuery.modifiedBefore = String(value);
      else
        diags.push(
          diagnostic(
            'unsupportedPredicate',
            'field "modified" supports only `within` / `before` (relative windows like "7d"); findFiles has no absolute-date filter for >/</>=/<=.',
            { backend: 'localFindFiles' }
          )
        );
      break;
    case 'accessed':
      if (f.op === 'within') toolQuery.accessedWithin = String(value);
      else diags.push(unsupportedField(f));
      break;
    case 'empty':
      toolQuery.empty = Boolean(value);
      break;
    case 'permissions':
      toolQuery.permissions = String(value);
      break;
    case 'executable':
    case 'readable':
    case 'writable':
      toolQuery[f.field] = Boolean(value);
      break;
    case 'entryType':
      toolQuery.entryType = String(value) === 'directory' ? 'd' : 'f';
      break;
    default:
      // Unmapped field: never silently drop the predicate — signal it so the
      // result is not mistaken for the unfiltered universe.
      diags.push(unsupportedField(f));
      break;
  }
  if (negate) {
    diags.push(
      diagnostic(
        'residualNotExact',
        'Negated field predicates over findFiles are best-effort.',
        { backend: 'localFindFiles', severity: 'warning' }
      )
    );
  }
}

function unsupportedField(f: FieldPredicate): OqlDiagnostic {
  return diagnostic(
    'unsupportedPredicate',
    `field "${f.field}" with operator "${f.op}" is not supported by the files backend.`,
    { backend: 'localFindFiles' }
  );
}

function searchControls(query: OqlQuery): Partial<LocalSearchToolQuery> {
  const out: Partial<LocalSearchToolQuery> = {};
  const s = query.controls?.search;
  if (s) {
    if (s.onlyMatching) out.onlyMatching = true;
    if (s.unique) out.unique = true;
    if (s.countUnique) out.countUnique = true;
    if (s.countMatchesPerFile) out.countMatchesPerFile = true;
    if (s.countLinesPerFile) out.countLinesPerFile = true;
    if (s.contextLines !== undefined) out.contextLines = s.contextLines;
    if (s.invertMatch) out.invertMatch = true;
    if (s.matchWindow !== undefined) out.matchWindow = s.matchWindow;
    if (s.matchContentLength !== undefined)
      out.matchContentLength = s.matchContentLength;
    if (s.maxMatchesPerFile !== undefined)
      out.maxMatchesPerFile = s.maxMatchesPerFile;
    if (s.matchPage !== undefined) out.matchPage = s.matchPage;
    // 'size'/'name' are files-lane (localFindFiles sortBy) sorts with no
    // localSearchCode equivalent — applyFindFilesSort handles them and
    // sortApplicabilityDiagnostics warns; only forward code-lane sorts here.
    if (s.sort && s.sort !== 'size' && s.sort !== 'name') out.sort = s.sort;
    if (s.sortReverse) out.sortReverse = true;
    // rankingProfile is a loose string in OQL controls; the localSearchCode
    // schema validates it against its language-profile enum at runtime.
    if (s.rankingProfile)
      out.rankingProfile =
        s.rankingProfile as LocalSearchToolQuery['rankingProfile'];
    if (s.debugRanking) out.debugRanking = true;
  }
  // budget.maxFiles is the search --max-files file cap; apply it even when
  // controls.search is absent (the old early-return dropped it silently).
  if (query.controls?.budget?.maxFiles !== undefined)
    out.maxFiles = query.controls.budget.maxFiles;
  return out;
}

function resultDiagnostics(
  result: { status?: string; error?: string; warnings?: string[] },
  backend: string
): OqlDiagnostic[] {
  const out: OqlDiagnostic[] = [];
  if (result.status === 'error') {
    out.push(
      diagnostic('invalidQuery', result.error ?? 'Backend error', { backend })
    );
  } else if (result.status === 'empty') {
    out.push(
      diagnostic('zeroMatches', 'Query ran and matched nothing.', {
        backend,
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  for (const w of result.warnings ?? []) {
    out.push(
      diagnostic(classifyWarning(w), w, {
        backend,
        severity: 'warning',
        blocksAnswer: false,
      })
    );
  }
  return out;
}

/** Map a backend warning string to the closest typed diagnostic code. */
function classifyWarning(
  message: string
): import('../types.js').DiagnosticCode {
  const m = message.toLowerCase();
  if (m.includes('skipped parsing') || m.includes('parse error')) {
    return 'partialParse';
  }
  if (m.includes('capped') || m.includes('truncat')) return 'matchTruncated';
  if (m.includes('redact') || m.includes('sanitiz') || m.includes('secret')) {
    return 'sanitized';
  }
  return 'partialResult';
}

function provenance(
  backend: string,
  source: QuerySource,
  where: Predicate | undefined
): OqlProvenance {
  return {
    backend,
    source,
    ...(where?.id ? { pushed: [where.id] } : {}),
  };
}
