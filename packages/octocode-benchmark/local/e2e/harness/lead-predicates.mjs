// Lead predicates shared by harness/lead-replay.mjs (live corpus replay) and
// harness/tool-health/driver.mjs (hermetic exploration against the mock).
//
// A lead is `{tool, query}` under `next.*` or `hints.*`. Each executed lead is
// judged by predicates chosen from its KEY FAMILY (docs/OCTOCODE_TOOLS.md,
// docs/TOOL_DATA_CONTRACT.md), never per fixture:
//   valid         the lead is accepted (no transport error, no invalid* row error)
//   executes      no error row (notFound, fileAccessFailed, ...)
//   channel       withheld evidence is a `next` page, never a `hints` lead
//   key-tool      the key's tool/operation matches (findCallers -> lspSearch callers)
//   no-brief      no lead inherits mainGoal/reasoning
//   extras        same-tool lead inherits extras; a cross-tool lead only `hints`
//   lifts-flag    include* keys set the flag they name (includeHidden:true, ...)
//   lifts-skip    an includeSkipped lead lifts at least one skip cause the source reported
//   keeps-scope   same-tool lead keeps every source field it does not mutate
//   ref-pinned    a lead ref equals the source ref or is a full commit SHA
//   non-empty     a lead that promises evidence returns some
//   fresh         a paging lead returns rows not already shown, or states terminalLimit
//   hits-match    a findText lead's hits match its own matchString under its regex mode
//   finds-anchor  a findText lead finds the source anchor text when its file holds it
//   covers-anchor a read lead shows a source anchor line or its matchString
//   anchor-line   an lspSearch lead's line holds its symbolName
//   not-build     a release-source lead is not a build-output path
//
// An entry is `{ sc, text, isError }`: `sc` the structured response (shared
// defaults expanded), `text` the raw text. File-system predicates resolve
// relative paths against `root` (the workspace the source call ran in).
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

// Key families.
export const PAGE_KEYS = /^(nextPage|nextMatchPage|nextFilePage|nextCommitPage|nextCommentPage|nextReviewPage|nextImporterPage|nextDiagnosticPage|continue[A-Za-z]*|expandScan|readHits|readDeclaration|wholeLines|readBoundedLines|responsePagination\.next)$/;
const FLAG_KEYS = { includeHidden: 'includeHidden', includeArchived: 'includeArchived' };
// The skip causes an includeSkipped lead may lift: the query flag that lifts each, and the words a source's hint or warning names it by.
const SKIP_CAUSES = [
  { cause: 'ignored', lifted: q => q.includeIgnored === true, named: /gitignore|\.ignore|ignored/i },
  { cause: 'hidden', lifted: q => q.includeHidden === true, named: /hidden|dot entr|['"(/\s]\.[\w-]+/i },
  { cause: 'default-excluded', lifted: q => q.defaultExcludes === false, named: /default|exclud|prune/i },
];
// Leads that promise evidence the source row is known to hold. find* leads may legitimately answer empty (the source found nothing either).
const NOT_EMPTY_KEYS = /^(read(Block|Files|Commit|Issue|PullRequest|AtCommit|AtMerge|MergeCommit|Parent|Patches|SelectedPatches|FullPatches)?|include(Skipped|Hidden|Archived)|narrowScope|view(Tree|Repo|Refs|ReleaseSource)|repair|ignoreCase)$/;
const SAME_TOOL_PAGE_KEYS = /^(next[A-Za-z]*Page|continue[A-Za-z]*|expandScan|restart)$/;
// Fields a same-tool lead may change (besides CONTINUATION_FIELDS), per key family.
const MUTABLE = [
  [/^include(Skipped|Hidden|Archived|BuildScripts)$/, new Set(['includeIgnored', 'includeHidden', 'includeArchived', 'defaultExcludes', 'rustContext'])],
  [/^expandScan$/, new Set(['maxEntries', 'maxFiles', 'maxDepth'])],
  // structureSearch widens a maxEntries cut through continueWalk.
  [/^continueWalk$/, new Set(['fullContent', 'maxEntries'])],
  [/^continue[A-Za-z]*$/, new Set(['fullContent'])],
];
// Enrichment and clipped-read leads add the option that widens the same read.
const ADDS_OPTIONS = /^(expand[A-Za-z]*|wholeLines|readBoundedLines|readHits|readDeclaration)$/;
const SAME_TOOL_KEYS = /^(includeSkipped|includeHidden|includeArchived|includeBuildScripts|narrowScope|restart|retry|repair|broadenSearch|ignoreCase)$/;
const CONTINUATION_FIELDS = new Set(['page', 'snapshot', 'scanOffset', 'matchPage', 'offset', 'importerPage', 'candidatePage', 'headSha', 'diagnosticPage', 'supersedes', 'materializeOffset', 'extras', 'mainGoal', 'reasoning', 'ranges', 'length', 'unit']);
const KEY_TOOLS = [
  [/^findText$/, { tools: ['localSearch', 'ghSearchCode', 'ghGetFileContent', 'localFetch'] }],
  [/^(findCallers|callers)$/, { tools: ['lspSearch'], operation: 'callers' }],
  [/^(findReferences|references)$/, { tools: ['lspSearch'], operation: 'references' }],
  [/^findCode$/, { tools: ['ghSearchCode'] }],
  [/^findRepository$/, { tools: ['ghSearchRepo'] }],
  [/^findPullRequest$/, { tools: ['ghSearchHistory'], operation: 'pullRequest' }],
  [/^findIssue$/, { tools: ['ghSearchHistory'], operation: 'issue' }],
  [/^findCommit$/, { tools: ['ghSearchHistory'], operation: 'commit' }],
  [/^readPullRequest$/, { tools: ['ghGetHistoryItem'], operation: 'pullRequest' }],
  [/^readIssue$/, { tools: ['ghGetHistoryItem'], operation: 'issue' }],
  [/^readCommit$/, { tools: ['ghGetHistoryItem', 'ghGetFileContent'] }],
  [/^viewTree$/, { tools: ['ghStructure', 'structureSearch', 'astSearch'], operations: { astSearch: 'syntaxTree' } }],
  [/^view(Refs|ReleaseSource)$/, { tools: ['ghStructure', 'structureSearch'] }],
  [/^viewRepo$/, { tools: ['ghStructure', 'ghSearchRepo'] }],
  [/^readDiagnostics$/, { tools: ['lspSearch'] }],
  [/^read(Block|Hits|Declaration|Files)?$/, { tools: ['localFetch', 'ghGetFileContent', 'ghGetHistoryItem'] }],
];
const BUILD_OUTPUT = /(^|\/)(dist|build|out|target|lib|coverage|\.next|node_modules)(\/|$)|\.min\.(js|css)$|\.map$/;
const SCOPE_FIELDS = new Set(['path', 'include', 'exclude', 'extensions', 'maxDepth', 'minDepth', 'maxEntries', 'maxFiles', 'fileHas', 'fileLacks', 'kinds', 'language', 'filename', 'entryType', 'nameRegex', 'pageSize', 'resultView', 'owner', 'repo']);
const PATH_FIELDS = new Set(['path', 'baseline', 'workspaceRoot']);

// Helpers.
export const short = (v, n = 240) => { const s = typeof v === 'string' ? v : JSON.stringify(v); return s.length > n ? `${s.slice(0, n - 1)}...` : s; };
export const isLead = node => node && typeof node === 'object' && typeof node.tool === 'string' && node.query && typeof node.query === 'object';
/** Decode a suggested call from a public hint; ordinary prose is not a call. */
export function suggestedCall(text) {
  if (typeof text !== 'string') return null;
  const match = /^(\w+): (Approximate: )?(\w+) (\{[\s\S]*\})$/.exec(text);
  if (!match) return null;
  try {
    const query = JSON.parse(match[4]);
    if (!Array.isArray(query.queries)) return null;
    return { key: match[1], tool: match[3], query, ...(match[2] ? { confidence: 'low' } : {}) };
  } catch { return null; }
}
export const rowOf = (query, i = 0) => query?.queries?.[i] ?? {};
const sortKeys = v => (Array.isArray(v) ? v.map(sortKeys) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sortKeys(v[k])])) : v);
/** Canonical JSON: object keys sorted at every level. */
export const canon = value => JSON.stringify(sortKeys(value));
const sameJson = (a, b) => canon(a) === canon(b);
/** Every property name a JSON schema declares, at any depth. */
export const properties = schema => { const out = new Set(); const walk = n => { if (!n || typeof n !== 'object') return; for (const [k, v] of Object.entries(n)) { if (k === 'properties' && v && typeof v === 'object') Object.keys(v).forEach(p => out.add(p)); walk(v); } }; walk(schema); return out; };

/** Every `{tool, query}` lead under `next.*` / `hints.*` (text tips excluded) with its row index. */
export function leadsOf(sc) {
  const out = [];
  const visit = (node, at, rowIndex) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach((c, i) => visit(c, `${at}[${i}]`, rowIndex)); return; }
    for (const [key, child] of Object.entries(node)) {
      if (key === 'hints' && Array.isArray(child)) {
        for (const text of child) {
          const lead = suggestedCall(text);
          if (lead) out.push({ ...lead, path: `${at}.hints.${lead.key}`, container: 'hints', rowIndex });
        }
      }
      if ((key === 'next' || key === 'hints') && child && typeof child === 'object' && !Array.isArray(child)) {
        for (const [name, lead] of Object.entries(child)) {
          if (name !== 'text' && isLead(lead)) out.push({ path: `${at}.${key}.${name}`, container: key, key: name, tool: lead.tool, query: lead.query, rowIndex });
        }
      }
      visit(child, `${at}.${key}`, rowIndex);
    }
  };
  (sc?.results ?? []).forEach((row, i) => visit(row, `.results[${i}]`, i));
  const envelope = sc?.responsePagination?.next;
  if (isLead(envelope)) out.push({ path: '.responsePagination.next', container: 'next', key: 'responsePagination.next', tool: envelope.tool, query: envelope.query, rowIndex: null });
  return out;
}

/** Evidence atoms of a result: every scalar outside pagination/hint metadata, keyed by its enclosing path. */
const NOISE = new Set(['next', 'hints', 'pagination', 'contentPagination', 'matchPagination', 'warnings', 'summary', 'snapshot', 'root', 'index', 'status', 'terminalLimit', 'coverage', 'completeness', 'complete', 'isPartial', 'partialReasons', 'atLeast', 'truncated', 'filesScanned', 'matchCount', 'matchedLineCount', 'fileCount', 'totalFiles', 'moreLines', 'currentPage', 'totalPages', 'totalItems', 'hasMore', 'pageSize', 'extras', 'echo', 'receipts', 'stats']);
export function atoms(sc) {
  const out = new Set();
  const walk = (node, ctx, key) => {
    if (node === null || node === undefined) return;
    if (Array.isArray(node)) { node.forEach(c => walk(c, ctx, key)); return; }
    if (typeof node === 'object') {
      const here = typeof node.path === 'string' ? node.path : typeof node.dir === 'string' ? node.dir : ctx;
      for (const [k, v] of Object.entries(node)) if (!NOISE.has(k)) walk(v, here, k);
      return;
    }
    if (typeof node === 'string') { for (const line of node.split('\n')) if (line.trim()) out.add(`${ctx}|${key}|${line}`); }
    else out.add(`${ctx}|${key}|${node}`);
  };
  for (const row of sc?.results ?? []) walk(row?.data ?? {}, '', '');
  return out;
}
export const terminal = entry => /terminalLimit"?:\s*true|terminal limit/i.test(JSON.stringify(entry.sc ?? '') + entry.text);

/** Anchor lines (per path) and symbol names in a source result. */
function anchorsOf(sc, rowIndex) {
  const lines = new Map();
  const texts = new Set();
  const add = (p, l) => { if (!lines.has(p)) lines.set(p, new Set()); lines.get(p).add(l); };
  const walk = (node, ctx) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(c => walk(c, ctx)); return; }
    const here = typeof node.path === 'string' ? node.path : ctx;
    if (Number.isInteger(node.line) && here) {
      add(here, node.line);
      if (Number.isInteger(node.endLine)) for (let l = node.line; l <= Math.min(node.endLine, node.line + 400); l++) add(here, l);
    }
    if (typeof node.symbolName === 'string') texts.add(node.symbolName);
    for (const [k, v] of Object.entries(node)) if (k !== 'next' && k !== 'hints') walk(v, here);
  };
  const rows = sc?.results ?? [];
  walk(rowIndex === null || rowIndex === undefined ? rows : rows[rowIndex], '');
  return { lines, texts };
}

/** Line numbers a result shows: numbered gutters ("12\ttext") and {line}/{start,end} objects. */
function shownLines(entry) {
  const found = new Set();
  const text = JSON.stringify(entry.sc?.results ?? '');
  for (const m of text.matchAll(/(?:\\n|"|^)(\d{1,7})(?:\\t|: |\|)/g)) found.add(Number(m[1]));
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) { n.forEach(walk); return; }
    if (Number.isInteger(n.line)) found.add(n.line);
    if (Number.isInteger(n.start) && Number.isInteger(n.end)) for (let l = n.start; l <= Math.min(n.end, n.start + 2000); l++) found.add(l);
    Object.values(n).forEach(walk);
  };
  walk(entry.sc?.results);
  return found;
}

/** Matchers for a query's matchString under its regex mode (smart-case literal by default). */
function matchersOf(q) {
  const list = Array.isArray(q.matchString) ? q.matchString : [q.matchString];
  return list.filter(x => typeof x === 'string').map(x => {
    if (q.regex === 'rust' || q.regex === 'pcre2') { try { return new RegExp(x, /[A-Z]/.test(x) ? '' : 'i'); } catch { return null; } }
    return { test: line => (x === x.toLowerCase() ? line.toLowerCase().includes(x) : line.includes(x)) };
  }).filter(Boolean);
}
const hitValues = sc => {
  const out = [];
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) { n.forEach(walk); return; }
    if (typeof n.value === 'string' && Number.isInteger(n.line)) out.push(n.value);
    for (const [k, v] of Object.entries(n)) if (k !== 'next' && k !== 'hints') walk(v);
  };
  walk(sc?.results);
  return out;
};

/** ok | empty | error of an entry, with its row error codes. */
export function outcomeOf(entry) {
  const rows = entry.sc?.results ?? [];
  // A page past the last (`outOfRange`) reads nothing, as an empty row.
  const statuses = rows.map(r => (r.status === 'outOfRange' ? 'empty' : r.status ?? 'ok'));
  const codes = rows.map(r => r.data?.errorCode).filter(Boolean);
  const error = entry.isError || statuses.includes('error');
  return { rows, codes, error, empty: rows.length > 0 && statuses.every(s => s === 'empty'), text: `${error ? 'error' : statuses.every(s => s === 'empty') ? 'empty' : 'ok'} ${codes.join(',')} ${entry.isError && !rows.length ? short(entry.text, 200) : ''}`.trim() };
}

const stripContinuation = q => Object.fromEntries(Object.entries(q).filter(([k]) => !CONTINUATION_FIELDS.has(k)));
const norm = (k, v, root) => (PATH_FIELDS.has(k) && typeof v === 'string' ? path.resolve(root, v) : v ?? null);
const pinnedRef = (k, src, v) => k === 'ref' && src === undefined && /^[0-9a-f]{40}$/.test(v); // pinning an unset ref to its SHA keeps the revision
/** Source fields a lead changed (paths resolved against `root`), ignoring `mutable` ones and, with `additions`, fields the source lacked. */
export function scopeChanges(src, lead, { mutable = new Set(), additions = false, root = process.cwd() } = {}) {
  const changed = [];
  for (const k of new Set([...Object.keys(stripContinuation(src)), ...Object.keys(stripContinuation(lead))])) {
    if (mutable.has(k) || pinnedRef(k, src[k], lead[k])) continue;
    if (additions && !(k in src)) continue;
    if (!sameJson(norm(k, src[k], root), norm(k, lead[k], root))) changed.push(`${k}: ${short(src[k] ?? null, 50)} -> ${short(lead[k] ?? null, 50)}`);
  }
  return changed;
}
// A count-view continuation materializes localSearch's default pageSize.
// Accept that one field only when the source page reports the same effective
// size, while comparing every other field against the original query.
export function sourceScopeForPage(source, lead, sq, lq) {
  // A whole-file read's window keeps the read's view as is: an omitted
  // `minify` (the exact view) stays omitted, a named one stays named. The
  // window may add fields the read lacked, so an omitted view is compared
  // explicitly.
  if (lead.tool === source.tool && lead.key === 'continueWindow' && sq.fullContent === true &&
      !Object.hasOwn(sq, 'minify')) return { ...sq, minify: null };
  if (source.tool !== 'localSearch' || lead.tool !== source.tool || lead.key !== 'nextPage' ||
      Object.hasOwn(sq, 'pageSize') || sq.resultView !== lq.resultView) return sq;
  const reported = source.entry.sc?.results?.[lead.rowIndex ?? 0]?.data?.pagination?.pageSize;
  return Number.isInteger(reported) && reported > 0 && lq.pageSize === reported
    ? { ...sq, pageSize: reported }
    : sq;
}
/** Whether the file or tree at `rel` (under `root`) holds `text` (smart case); null when unreadable. */
function fileHolds(rel, text, root) {
  const abs = path.isAbsolute(rel) ? rel : path.join(root, rel);
  try {
    const smart = text === text.toLowerCase(); // smart-case: an uppercase letter makes the match case-sensitive
    if (fs.statSync(abs).isFile()) return smart ? fs.readFileSync(abs, 'utf8').toLowerCase().includes(text) : fs.readFileSync(abs, 'utf8').includes(text);
    const run = spawnSync('grep', [smart ? '-rIlFi' : '-rIlF', '--exclude-dir=node_modules', '--exclude-dir=.git', '--exclude-dir=target', '--exclude-dir=build', '--exclude-dir=dist', '--exclude=*.log', text, abs], { encoding: 'utf8', timeout: 10_000 });
    return run.stdout.split('\n').filter(Boolean).some(f => !path.relative(abs, f).split(path.sep).some(seg => seg.startsWith('.')));
  } catch { return null; }
}

/**
 * Judge one executed lead against the predicates of its key family.
 * `source` is `{ tool, args, entry }` (the call that emitted the lead), `entry`
 * the lead's own result, `shown` the evidence atoms already shown on its chain.
 * `root` resolves relative paths; `propsOf(tool)` lists the tool's accepted
 * query fields (for lifts-flag). Returns `[{ assertion, ok, detail }]`.
 */
export function judgeLead(lead, source, entry, shown, { walked = false, root = process.cwd(), propsOf = () => new Set(), defaultsOf = () => ({}) } = {}) {
  const results = [];
  const record = (assertion, ok, detail) => { results.push({ assertion, ok: !!ok, detail }); return ok; };
  const out = outcomeOf(entry);
  const lq = rowOf(lead.query);
  const sq = rowOf(source.args, lead.rowIndex ?? 0);
  const single = (lead.query.queries ?? []).length === 1;
  const sameTool = lead.tool === source.tool;
  const { key } = lead;

  const invalid = (entry.isError && !out.rows.length) || out.codes.some(c => /^invalid/.test(c));
  record('valid', !invalid, out.text);
  if (key !== 'retry') record('executes', invalid || !out.error, out.text);

  // Channel: withheld evidence is a `next` page; a lead lives under `hints`.
  if (!walked && PAGE_KEYS.test(key) && key !== 'responsePagination.next') record('channel', lead.container === 'next', `${key} is a page but sits under ${lead.container}`);

  const rule = KEY_TOOLS.find(([re]) => re.test(key))?.[1];
  if (rule && !walked) {
    const operation = rule.operation ?? rule.operations?.[lead.tool];
    record('key-tool', rule.tools.includes(lead.tool) && (!operation || lq.operation === operation), `${lead.tool} operation=${lq.operation ?? '-'}; ${key} expects ${rule.tools.join('|')}${operation ? ` ${operation}` : ''}`);
  }
  if (!walked && (SAME_TOOL_KEYS.test(key) || SAME_TOOL_PAGE_KEYS.test(key))) record('key-tool', sameTool || (key === 'includeArchived' && lead.tool === 'ghSearchRepo'), `${lead.tool} lead from a ${source.tool} row`);

  // Request-level fields (TOOL_DATA_CONTRACT): no continuation inherits mainGoal/reasoning; a same-tool lead inherits extras, a cross-tool lead only `hints`.
  if (single && !walked && lead.tool !== 'clasify') {
    record('no-brief', !('mainGoal' in lq) && !('reasoning' in lq), `lead carries mainGoal=${short(lq.mainGoal ?? null, 40)} reasoning=${short(lq.reasoning ?? null, 40)}`);
    record('extras', sameTool ? (sq.extras ?? []).every(e => (lq.extras ?? []).includes(e)) : (lq.extras ?? []).every(e => e === 'hints'), `lead extras=${short(lq.extras ?? null)} source extras=${short(sq.extras ?? null)} ${sameTool ? 'same tool' : 'cross tool'}`);
  }

  // include* keys set the flag they name.
  const flag = FLAG_KEYS[key];
  if (flag && single && propsOf(lead.tool).has(flag)) {
    record('lifts-flag', lq[flag] === true, `${flag}=${lq[flag] ?? 'unset'} includeHidden=${lq.includeHidden ?? 'unset'} defaultExcludes=${lq.defaultExcludes ?? 'unset'}`);
  }

  // includeSkipped lifts at least one skip cause, and one the source named
  // when its hints or warnings name any (the flag set is the minimal one the
  // probe found, so it may lift several).
  if (key === 'includeSkipped' && single) {
    const lifted = SKIP_CAUSES.filter(c => c.lifted(lq) && !c.lifted(sq));
    const said = outcomeOf(source.entry).text;
    const reported = SKIP_CAUSES.filter(c => c.named.test(said));
    const ok = lifted.length > 0 && (reported.length === 0 || lifted.some(c => reported.includes(c)));
    record('lifts-skip', ok, `lifted=${lifted.map(c => c.cause).join('+') || 'none'} reported=${reported.map(c => c.cause).join('+') || 'none'}`);
  }

  // A same-tool lead keeps everything it does not mutate.
  if (single && sameTool && (flag || key === 'includeSkipped' || PAGE_KEYS.test(key) || /^(restart|retry)$/.test(key))) {
    const mutable = MUTABLE.find(([re]) => re.test(key))?.[1];
    const sourceScope = sourceScopeForPage(source, lead, sq, lq);
    const changed = scopeChanges({ ...defaultsOf(source.tool, sourceScope), ...sourceScope }, { ...defaultsOf(lead.tool, lq), ...lq }, { mutable, additions: ADDS_OPTIONS.test(key) || /^continue/.test(key), root });
    record('keeps-scope', changed.length === 0, changed.join('; '));
  }
  // narrowScope keeps the question (what is searched) and changes where it looks.
  if (single && sameTool && key === 'narrowScope') {
    // structureSearch narrows by listing subdirectories: its operation picks the listing shape, not the question.
    const scope = lead.tool === 'structureSearch' ? new Set([...SCOPE_FIELDS, 'operation']) : SCOPE_FIELDS;
    const changed = scopeChanges(sq, lq, { mutable: scope, additions: true, root });
    const narrowed = [...scope].some(k => !sameJson(norm(k, sq[k], root), norm(k, lq[k], root)));
    record('keeps-scope', changed.length === 0 && narrowed, changed.length ? changed.join('; ') : narrowed ? '' : 'narrowScope changes no scope field');
  }

  // A lead ref is the source ref or a pinned SHA (or a value the source result carries).
  if (single && typeof lq.ref === 'string') {
    const pinned = lq.ref === sq.ref || /^[0-9a-f]{40}$/.test(lq.ref) || JSON.stringify(source.entry.sc ?? '').includes(lq.ref);
    record('ref-pinned', pinned, `ref=${lq.ref} source ref=${sq.ref ?? 'unset'}`);
  }

  if (!out.error && NOT_EMPTY_KEYS.test(key)) record('non-empty', !out.empty, out.text);

  // Paging: rows not already shown, or an explicit terminal limit.
  if (PAGE_KEYS.test(key) && key !== 'responsePagination.next' && !out.error) {
    const now = atoms(entry.sc);
    const fresh = [...now].filter(a => !shown.has(a)).length;
    record('fresh', fresh > 0 || terminal(entry), `${fresh} new of ${now.size} evidence atoms; terminalLimit=${terminal(entry)}`);
  }

  if (key === 'findText' && !out.error) {
    const hits = lead.tool === 'localSearch' ? hitValues(entry.sc) : [];
    const matchers = matchersOf(lq);
    if (hits.length && matchers.length) record('hits-match', hits.every(v => matchers.some(m => m.test(v))), `${hits.length} hits; first unmatched: ${short(hits.find(v => !matchers.some(m => m.test(v))) ?? '', 100)}; matchString=${short(lq.matchString ?? '', 80)} regex=${lq.regex ?? 'literal'}`);
    const anchor = typeof sq.symbolName === 'string' ? sq.symbolName : typeof sq.matchString === 'string' && sq.regex !== 'rust' && sq.regex !== 'pcre2' ? sq.matchString : null;
    const held = anchor && lq.path ? fileHolds(lq.path, anchor, root) : null;
    if (held) record('finds-anchor', !out.empty, `${lq.path} holds ${short(anchor, 60)} but the lead found nothing: matchString=${short(lq.matchString ?? '', 80)} regex=${lq.regex ?? 'literal'}`);
  }

  // A read lead shows an anchor line of its source row, or its matchString.
  if (/^read[A-Za-z]*$/.test(key) && !PAGE_KEYS.test(key) && ['localFetch', 'ghGetFileContent'].includes(lead.tool) && !out.error && single) {
    const anchors = anchorsOf(source.entry.sc, lead.rowIndex).lines.get(lq.path);
    if (anchors?.size) {
      const shownNow = shownLines(entry);
      const text2 = JSON.stringify(entry.sc?.results ?? '');
      const ok = [...anchors].some(l => shownNow.has(l)) || matchersOf(lq).some(m => m.test(text2));
      record('covers-anchor', ok, `anchors ${[...anchors].slice(0, 6)} not shown; lead ranges=${short(lq.ranges ?? null)} matchString=${short(lq.matchString ?? null)}`);
    }
  }

  // lspSearch anchors: the line holds the symbol.
  if (lead.tool === 'lspSearch' && single && lq.path && Number.isInteger(lq.line) && typeof lq.symbolName === 'string') {
    try {
      const file = fs.readFileSync(path.isAbsolute(lq.path) ? lq.path : path.join(root, lq.path), 'utf8').split('\n');
      const name = lq.symbolName.split(/[.:]+/).pop();
      record('anchor-line', (file[lq.line - 1] ?? '').includes(name), `${lq.path}:${lq.line} is ${short(file[lq.line - 1] ?? '<past end>', 100)}; symbolName=${lq.symbolName}`);
    } catch { /* remote or unreadable path: nothing to compare */ }
  }

  if (key === 'viewReleaseSource' && single) {
    const where = [lq.path, ...(lq.include ?? [])].filter(p => typeof p === 'string');
    record('not-build', !where.some(p => BUILD_OUTPUT.test(p)), `path=${lq.path ?? '-'} include=${short(lq.include ?? null)} ref=${lq.ref ?? '-'}`);
  }
  return results;
}
