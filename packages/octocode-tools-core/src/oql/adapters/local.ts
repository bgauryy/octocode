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
import type {
  FieldPredicate,
  OqlDiagnostic,
  OqlProvenance,
  OqlQuery,
  Predicate,
  QueryScope,
  QuerySource,
} from '../types.js';

export interface AdapterResult extends MappedResult {
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
}

/** Resolve the local filesystem root for a query (local or materialized). */
function localRoot(query: OqlQuery): string {
  if (query.from?.kind === 'local') return query.from.path;
  if (query.from?.kind === 'materialized') return query.from.localPath;
  throw new Error('localExecute requires a local or materialized source.');
}

function firstScopePath(scope: QueryScope | undefined): string | undefined {
  if (!scope?.path) return undefined;
  return Array.isArray(scope.path) ? scope.path[0] : scope.path;
}

function scopeToCommon(scope: QueryScope | undefined): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (scope?.include) out.include = scope.include;
  if (scope?.exclude) out.exclude = scope.exclude;
  if (scope?.excludeDir) out.excludeDir = scope.excludeDir;
  if (scope?.hidden !== undefined) out.hidden = scope.hidden;
  if (scope?.noIgnore !== undefined) out.noIgnore = scope.noIgnore;
  return out;
}

function languageInclude(scope: QueryScope | undefined): string | undefined {
  const lang = scope?.language;
  if (!lang) return undefined;
  return Array.isArray(lang) ? lang[0] : lang;
}

export async function executeLocal(query: OqlQuery): Promise<AdapterResult> {
  // dispatch only routes local/materialized code/content/structure/files here.
  const source = query.from as QuerySource;
  const root = localRoot(query);
  const scopePath = firstScopePath(query.scope);
  const searchPath = scopePath ? path.join(root, scopePath) : root;

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
  // not(leaf) compiles to a single invertMatch call; not(boolean) does not.
  if (p.kind === 'not') return isBooleanPredicate(p.predicate);
  return false;
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
  const langType = m.langType ?? languageInclude(query.scope);
  const toolQuery: Record<string, unknown> = {
    path: searchPath,
    ...scopeToCommon(query.scope),
    ...(langType ? { langType } : {}),
    ...(query.view === 'discovery' ? { filesOnly: true } : {}),
    ...(query.view === 'detailed' ? { contextLines: 3 } : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
    ...searchControls(query),
  };

  if (m.mode === 'structural') {
    toolQuery.mode = 'structural';
    if (m.pattern !== undefined) toolQuery.pattern = m.pattern;
    if (m.rule !== undefined) toolQuery.rule = m.rule;
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

  const result = await searchContentRipgrep(toolQuery as never);
  const mapped = mapCodeResult(result, source);
  const diagnostics = resultDiagnostics(result, 'localSearchCode');

  // Per-file match truncation: localSearchCode caps matches per file (default
  // ~10). If any file reports more matches than it returned, the result is NOT
  // complete proof — surface matchTruncated (blocking) so evidence drops to
  // partial, and point at the per-file matchPage continuation.
  const truncatedFiles = matchTruncatedFiles(result);
  if (truncatedFiles.length > 0) {
    const total = truncatedFiles.reduce((n, f) => n + (f.total ?? 0), 0);
    const shown = truncatedFiles.reduce((n, f) => n + f.shown, 0);
    diagnostics.push(
      diagnostic(
        'matchTruncated',
        `${truncatedFiles.length} file(s) had more matches than returned (showed ${shown} of ${total}); raise controls.search.maxMatchesPerFile or page with controls.search.matchPage.`,
        {
          backend: 'localSearchCode',
          repair: {
            message:
              'Set controls.search.maxMatchesPerFile higher, or follow next.matchPage per file.',
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

  const toolQuery: Record<string, unknown> = {
    path: searchPath,
    details: true,
    showFileLastModified: true,
    ...(query.scope?.excludeDir ? { excludeDir: query.scope.excludeDir } : {}),
    ...(query.scope?.maxDepth !== undefined
      ? { maxDepth: query.scope.maxDepth }
      : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
  };

  const fieldDiags: OqlDiagnostic[] = [];
  if (where) {
    applyFieldPredicate(where, toolQuery, fieldDiags);
  }
  // language scope -> name glob
  const lang = languageInclude(query.scope);
  if (lang && !toolQuery.names && !toolQuery.pathPattern) {
    toolQuery.names = [`*.${lang}`];
  }

  const result = await findFiles(toolQuery as never);
  const mapped = mapFilesResult(result, source);
  return {
    ...mapped,
    diagnostics: [
      ...fieldDiags,
      ...resultDiagnostics(result, 'localFindFiles'),
    ],
    provenance: [provenance('localFindFiles', source, where)],
  };
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
    searchPath,
    provenance,
    diagnostics
  );
  const rows = [...set].sort().map<{
    kind: 'file';
    source: QuerySource;
    path: string;
    entryType: 'file';
  }>(p => ({ kind: 'file', source, path: p, entryType: 'file' }));
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

async function evalFilesPredicate(
  query: OqlQuery,
  p: Predicate,
  searchPath: string,
  prov: OqlProvenance[],
  diags: OqlDiagnostic[]
): Promise<Set<string>> {
  switch (p.kind) {
    case 'all': {
      const sets = await Promise.all(
        p.of.map(c => evalFilesPredicate(query, c, searchPath, prov, diags))
      );
      return sets.reduce((acc, s) =>
        acc === undefined ? s : intersect(acc, s)
      );
    }
    case 'any': {
      const sets = await Promise.all(
        p.of.map(c => evalFilesPredicate(query, c, searchPath, prov, diags))
      );
      const out = new Set<string>();
      for (const s of sets) for (const v of s) out.add(v);
      return out;
    }
    case 'not': {
      const universe = await universeFiles(query, searchPath, prov);
      const inner = await evalFilesPredicate(
        query,
        p.predicate,
        searchPath,
        prov,
        diags
      );
      return new Set([...universe].filter(f => !inner.has(f)));
    }
    default:
      return leafFiles(query, p, searchPath, prov, diags);
  }
}

function intersect(a: Set<string>, b: Set<string>): Set<string> {
  return new Set([...a].filter(v => b.has(v)));
}

/** All files in scope — the candidate universe for negation. */
async function universeFiles(
  query: OqlQuery,
  searchPath: string,
  prov: OqlProvenance[]
): Promise<Set<string>> {
  const lang = languageInclude(query.scope);
  const result = await findFiles({
    path: searchPath,
    entryType: 'f',
    ...(query.scope?.excludeDir ? { excludeDir: query.scope.excludeDir } : {}),
    ...(lang ? { names: [`*.${lang}`] } : {}),
  } as never);
  prov.push({ backend: 'localFindFiles', source: query.from });
  // content-predicate negation applies to files, not directory entries
  return new Set(
    (result.files ?? [])
      .filter(f => f.type === undefined || f.type === 'f' || f.type === 'file')
      .map(f => f.path)
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
  if (leaf.kind === 'field') {
    const tq: Record<string, unknown> = { path: searchPath, details: true };
    applyFieldPredicate(leaf, tq, diags);
    const r = await findFiles(tq as never);
    prov.push({ backend: 'localFindFiles', source: query.from });
    return new Set((r.files ?? []).map(f => f.path));
  }
  // content leaf (text/regex/structural) -> filesOnly search
  const compiled = compileWhere(leaf);
  if (compiled.unsupported) {
    diags.push(
      diagnostic(compiled.unsupported.code, compiled.unsupported.message, {
        backend: 'localSearchCode',
      })
    );
    return new Set();
  }
  const m = compiled.match!;
  const lang = m.langType ?? languageInclude(query.scope);
  const tq: Record<string, unknown> = {
    path: searchPath,
    filesOnly: true,
    maxFiles: 100000,
    ...scopeToCommon(query.scope),
    ...(lang ? { langType: lang } : {}),
  };
  if (m.mode === 'structural') {
    tq.mode = 'structural';
    if (m.pattern !== undefined) tq.pattern = m.pattern;
    if (m.rule !== undefined) tq.rule = m.rule;
  } else {
    tq.keywords = m.keywords;
    if (m.fixedString) tq.fixedString = true;
    if (m.perlRegex) tq.perlRegex = true;
    if (m.caseSensitive) tq.caseSensitive = true;
    if (m.caseInsensitive) tq.caseInsensitive = true;
    if (m.wholeWord) tq.wholeWord = true;
  }
  const r = await searchContentRipgrep(tq as never);
  prov.push({ backend: 'localSearchCode', source: query.from });
  return new Set((r.files ?? []).map(f => f.path));
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
        .reduce((acc, s) => (acc === undefined ? s : intersect(acc, s)));
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
  const lang = m.langType ?? languageInclude(query.scope);
  const tq: Record<string, unknown> = {
    path: searchPath,
    ...scopeToCommon(query.scope),
    ...(lang ? { langType: lang } : {}),
  };
  if (m.mode === 'structural') {
    tq.mode = 'structural';
    if (m.pattern !== undefined) tq.pattern = m.pattern;
    if (m.rule !== undefined) tq.rule = m.rule;
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
  const r = await searchContentRipgrep(tq as never);
  prov.push({ backend: 'localSearchCode', source });
  return mapCodeResult(r, source)
    .results as import('../types.js').OqlCodeResultRow[];
}

async function executeStructure(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const toolQuery: Record<string, unknown> = {
    path: searchPath,
    // details:true makes the tool emit per-entry rows (with depth/size) rather
    // than grouped files/folders lists, which map cleanly to tree rows.
    details: true,
    ...(query.fetch?.tree?.maxDepth !== undefined
      ? { maxDepth: query.fetch.tree.maxDepth, recursive: true }
      : {}),
    ...(query.scope?.hidden !== undefined
      ? { hidden: query.scope.hidden }
      : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
  };
  const result = await viewStructure(toolQuery as never);
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
  const toolQuery: Record<string, unknown> = {
    path: searchPath,
    minify,
    ...(c?.range?.startLine !== undefined
      ? { startLine: c.range.startLine }
      : {}),
    ...(c?.range?.endLine !== undefined ? { endLine: c.range.endLine } : {}),
    ...(c?.range?.contextLines !== undefined
      ? { contextLines: c.range.contextLines }
      : {}),
    ...(c?.match?.text !== undefined ? { matchString: c.match.text } : {}),
    ...(c?.match?.regex ? { matchStringIsRegex: true } : {}),
    ...(c?.match?.caseSensitive ? { matchStringCaseSensitive: true } : {}),
    ...(c?.charOffset !== undefined ? { charOffset: c.charOffset } : {}),
    ...(c?.charLength !== undefined ? { charLength: c.charLength } : {}),
    ...(c?.fullContent ? { fullContent: true } : {}),
  };
  const result = await fetchContent(toolQuery as never);
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

function isContentPredicate(p: Predicate): boolean {
  if (p.kind === 'text' || p.kind === 'regex' || p.kind === 'structural') {
    return true;
  }
  if (p.kind === 'not') return isContentPredicate(p.predicate);
  return false;
}

function applyFieldPredicate(
  where: Predicate,
  toolQuery: Record<string, unknown>,
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
    case 'extension':
      toolQuery.names = [`*.${String(value).replace(/^\./, '')}`];
      break;
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
      else
        diags.push(
          diagnostic(
            'unsupportedPredicate',
            'field "modified" supports only `within` (relative window like "7d"); findFiles has no absolute-date filter for >/</>=/<=.',
            { backend: 'localFindFiles' }
          )
        );
      break;
    case 'entryType':
      toolQuery.entryType = String(value) === 'directory' ? 'd' : 'f';
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

function searchControls(query: OqlQuery): Record<string, unknown> {
  const s = query.controls?.search;
  if (!s) return {};
  const out: Record<string, unknown> = {};
  if (s.onlyMatching) out.onlyMatching = true;
  if (s.unique) out.unique = true;
  if (s.countUnique) out.countUnique = true;
  if (s.countMatchesPerFile) out.countMatchesPerFile = true;
  if (s.countLinesPerFile) out.countLinesPerFile = true;
  if (s.matchWindow !== undefined) out.matchWindow = s.matchWindow;
  if (s.matchContentLength !== undefined)
    out.matchContentLength = s.matchContentLength;
  if (s.maxMatchesPerFile !== undefined)
    out.maxMatchesPerFile = s.maxMatchesPerFile;
  if (s.matchPage !== undefined) out.matchPage = s.matchPage;
  if (s.sort) out.sort = s.sort;
  if (s.sortReverse) out.sortReverse = true;
  if (s.rankingProfile) out.rankingProfile = s.rankingProfile;
  if (s.debugRanking) out.debugRanking = true;
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
