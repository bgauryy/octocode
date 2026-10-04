// Response sensors shared by the competitor harness (RFC tool-quality-efficiency
// S1): continuation (hint) walking, schema-error counting, the verbose-field
// report, body hash + key byte shares, and the byte-regression gate (pinned
// anchor plus rolling baseline). Pure functions only; `--self-test` runs them
// on synthetic responses.
//
// Two continuation containers (core `CONTINUATION_CHANNELS`):
//   next   pages: continuations that reach unshown data of the same result,
//          plus the envelope responsePagination.next.
//   hints  `{ text?: string[], <leadName>: {tool, query} }`: prose tips and
//          optional lead calls.
// A lead is any `hints` entry, or (in legacy streams, where `next` also held
// leads and `hints` was a prose string[]) a `next` entry that core classifies
// as a lead. Briefs (goal / mainGoal / reasoning) may be absent from any query.
import { continuationChannel } from '@octocodeai/config/schema';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

// ---------- continuations ----------
export const HINT_KEYS = ['hints', 'next'];
export const isHintKey = key => HINT_KEYS.includes(key);
/** Brief and presentation fields: never part of a continuation's identity. */
export const BRIEF_KEYS = ['goal', 'mainGoal', 'reasoning', 'debug'];
export const isHint = v => !!v && typeof v === 'object' && !Array.isArray(v) && typeof v.tool === 'string' && v.query && typeof v.query === 'object';
/** `{name, hint}` for each executable entry of a continuation container (a map of named hints, an array, or one hint). */
export function hintEntries(container, name = 'hints') {
  if (!container || typeof container !== 'object') return [];
  if (isHint(container)) return [{ name, hint: container }];
  if (Array.isArray(container)) return container.filter(isHint).map(hint => ({ name, hint }));
  return Object.entries(container).filter(([, v]) => isHint(v)).map(([k, hint]) => ({ name: k, hint }));
}
/** The envelope continuation container of a response (`responsePagination.next`; `.hints` tolerated). */
export const envelopeContainer = sc => sc?.responsePagination?.next ?? sc?.responsePagination?.hints;
/** True when a `hints`/`next` value holds executable continuations (error rows carry `hints` as prose strings). */
export const isHintContainer = (key, value) => isHintKey(key) && hintEntries(value).length > 0;
/** The emitting tool of a structured response when the caller does not know it: clasify answers carry `queries`. */
export const responseTool = sc => (Array.isArray(sc?.queries) ? 'clasify' : '');
/** True when core classifies continuation `name` of `tool` as a page. */
export const isPageName = (name, tool = '') => continuationChannel(tool, name) === 'page';
/** Lead entries offered by one object: all of its `hints` entries plus lead-named `next` entries (legacy shape). */
export function leadEntries(holder, tool = '') {
  if (!holder || typeof holder !== 'object' || Array.isArray(holder)) return [];
  return [...hintEntries(holder.hints), ...hintEntries(holder.next).filter(e => !isPageName(e.name, tool))];
}
/** Largest lead menu (the R2 cap applies to leads; `next` pages are uncapped) under a node. */
export function maxLeadEntries(node, tool = responseTool(node)) {
  let max = 0;
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    if (!Array.isArray(n)) max = Math.max(max, leadEntries(n, tool).length);
    for (const child of Object.values(n)) walk(child);
  };
  walk(node);
  return max;
}
/** Page continuations of a response: page-named `next` entries, plus the envelope `responsePagination.next`. */
export function pageEntries(sc, tool = responseTool(sc)) {
  const out = [];
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    for (const [key, child] of Object.entries(n)) {
      if (key === 'next') out.push(...hintEntries(child, 'next').filter(e => isPageName(e.name, tool)));
      else walk(child);
    }
  };
  walk(sc);
  return out;
}
/** JSON bytes under every occurrence of one key (key included). */
export function bytesUnderKey(value, keyName) {
  let total = 0;
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    for (const [key, child] of Object.entries(node)) {
      if (key === keyName && child && typeof child === 'object') { total += JSON.stringify(child).length + key.length + 3; continue; }
      walk(child);
    }
  };
  walk(value);
  return total;
}
/** JSON bytes of lead entries wherever they live (comparable across the next → hints move). */
export function leadBytes(value, tool = responseTool(value)) {
  let total = 0;
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    if (!Array.isArray(n)) for (const e of leadEntries(n, tool)) total += JSON.stringify(e.hint).length + e.name.length + 3;
    for (const child of Object.values(n)) walk(child);
  };
  walk(value);
  return total;
}

// ---------- schema errors ----------
/** Row error codes that mean the input did not validate (as opposed to a runtime miss). */
export const SCHEMA_ERROR_CODES = new Set(['invalidInput', 'invalidQuery', 'validation', 'invalidPagination', 'invalidCursor']);
const CALL_VALIDATION = /Input validation error|Invalid arguments for tool|MCP error -32602/i;
/**
 * Validation errors in one call: a whole-call rejection (MCP SDK input
 * validation, or a CLI usage error with `details`) counts 1; otherwise each
 * row whose errorCode is a schema code counts 1. Returns {count, codes}.
 */
export function schemaErrors(entry) {
  const raw = entry?.raw;
  if (entry?.isError && (!raw || typeof raw !== 'object' || !Array.isArray(raw.results)) && (CALL_VALIDATION.test(String(entry.text ?? '').slice(0, 400)) || Array.isArray(raw?.details))) {
    return { count: 1, codes: ['callValidation'], detail: String(entry.text ?? '').slice(0, 240) };
  }
  const codes = [];
  for (const row of Array.isArray(raw?.results) ? raw.results : []) {
    const code = row?.data?.errorCode ?? row?.errorCode;
    if (row?.status === 'error' && SCHEMA_ERROR_CODES.has(code)) codes.push(code);
  }
  return { count: codes.length, codes, detail: codes.length ? String(entry.text ?? '').slice(0, 240) : '' };
}

// ---------- body hash + key byte shares ----------
const size = v => (v === undefined ? 0 : JSON.stringify(v).length);
/** sha256 (16 hex) over every response body of a task, in call order. */
export function bodyHash(entries) {
  const h = createHash('sha256');
  for (const e of entries) h.update(`${e.tool}\n${e.raw ? JSON.stringify(e.raw) : String(e.text ?? '')}\n`);
  return h.digest('hex').slice(0, 16);
}
/**
 * Bytes per top-level key, summed over a task's responses: envelope keys as
 * `<key>`, row keys as `results[].<key>`, row data keys as `data.<key>`.
 * Enough to name the key behind a regression (L01 grew by `data.warnings`).
 */
export function keyBytes(entries) {
  const out = {};
  const add = (key, value) => { out[key] = (out[key] ?? 0) + size(value) + key.length + 3; };
  for (const e of entries) {
    const sc = e.raw;
    if (!sc || typeof sc !== 'object') { out['(text)'] = (out['(text)'] ?? 0) + String(e.text ?? '').length; continue; }
    for (const [key, value] of Object.entries(sc)) {
      if (key !== 'results' || !Array.isArray(value)) { add(key, value); continue; }
      for (const row of value) {
        if (!row || typeof row !== 'object') continue;
        for (const [rk, rv] of Object.entries(row)) {
          if (rk === 'data' && rv && typeof rv === 'object' && !Array.isArray(rv)) for (const [dk, dv] of Object.entries(rv)) add(`data.${dk}`, dv);
          else add(`results[].${rk}`, rv);
        }
      }
    }
  }
  return Object.fromEntries(Object.entries(out).sort((a, b) => b[1] - a[1]));
}
export function keyShares(bytesByKey) {
  const total = Object.values(bytesByKey).reduce((a, b) => a + b, 0);
  return Object.fromEntries(Object.entries(bytesByKey).map(([k, b]) => [k, total ? +(b / total).toFixed(3) : 0]));
}
/** The keys that grew most against a baseline's key bytes: "data.warnings +18,400 B". */
export function keyGrowth(now = {}, was = {}, top = 2) {
  return Object.keys({ ...now, ...was })
    .map(k => ({ key: k, delta: (now[k] ?? 0) - (was[k] ?? 0) }))
    .filter(x => x.delta > 0)
    .sort((a, b) => b.delta - a.delta)
    .slice(0, top)
    .map(x => `${x.key} +${x.delta} B`);
}

// ---------- verbose-field report ----------
export const VERBOSE_FIELDS_FILE = path.join(HERE, 'verbose-fields.json');
export function loadVerboseRules(file = VERBOSE_FIELDS_FILE) {
  const { rules } = JSON.parse(fs.readFileSync(file, 'utf8'));
  for (const r of rules) if (!VERBOSE_CHECKS[r.check]) throw new Error(`verbose-fields: unknown check ${r.check} (${r.id})`);
  return rules;
}
/**
 * Predicates over (value of rule.key, the object holding it, context). Each
 * returns the default-output bytes the rule attributes to that occurrence (0 = no hit).
 * Context: inHint (inside a continuation), hints (the row's continuation
 * queries), query (the row's input query).
 */
const VERBOSE_CHECKS = {
  // A pagination object that says nothing remains.
  hasMoreFalse: value => (value && typeof value === 'object' && value.hasMore === false ? size(value) : 0),
  // Pagination fields a sibling continuation already carries verbatim.
  duplicatesHint: (value, holder, ctx, { fields = [], queryKeys = {} } = {}) => {
    if (!value || typeof value !== 'object') return 0;
    let bytes = 0;
    for (const f of fields) {
      if (value[f] == null) continue;
      const qk = queryKeys[f] ?? f;
      if (ctx.hints.some(q => q?.[qk] === value[f])) bytes += size(value[f]) + f.length + 3;
    }
    return bytes;
  },
  // A list entry that inlines a long non-evidence list (skipped-file names).
  longListEntry: (value, holder, ctx, { maxBytes = 300, maxNames = 5 } = {}) => (Array.isArray(value)
    ? value.filter(s => typeof s === 'string' && (s.length > maxBytes || s.split(/,\s*/).length > maxNames)).reduce((a, s) => a + s.length + 2, 0)
    : 0),
  // Present inside a continuation (duplicates data the row already shows).
  inHint: (value, holder, ctx) => (ctx.inHint ? size(value) : 0),
  // An object field whose value a sibling list already holds verbatim.
  repeatsSibling: (value, holder, ctx, { field, sibling } = {}) => {
    const repeated = value?.[field];
    const list = holder?.[sibling];
    return repeated != null && (Array.isArray(list) ? list : [list]).some(item => item === repeated) ? size(repeated) + field.length + 3 : 0;
  },
  // A continuation field with a fixed value.
  equalsInHint: (value, holder, ctx, { equals } = {}) => (ctx.inHint && value === equals ? size(value) + 3 : 0),
  // An object that only echoes the row's input.
  echoesQuery: (value, holder, ctx, { fields = [] } = {}) => (value && typeof value === 'object' && fields.length && fields.every(([rk, qk]) => value[rk] != null && value[rk] === ctx.query?.[qk]) ? size(value) : 0),
};
/**
 * Default-output fields from the verbose list in one response. Rows queried
 * with `debug:true` are skipped (the sensor measures `debug:false` output).
 * Returns {ruleId: {count, bytes}} plus `debugRows`.
 */
export function verboseFields(entry, rules) {
  const out = {};
  const sc = entry?.raw;
  if (!sc || typeof sc !== 'object') return out;
  const queries = Array.isArray(entry.args?.queries) ? entry.args.queries : [entry.args ?? {}];
  const hit = (rule, bytes) => { if (bytes > 0) { const r = (out[rule.id] ??= { count: 0, bytes: 0 }); r.count += 1; r.bytes += bytes; } };
  const visit = (node, ctx) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { for (const child of node) visit(child, ctx); return; }
    for (const [key, value] of Object.entries(node)) {
      for (const rule of rules) if (rule.key === key) hit(rule, VERBOSE_CHECKS[rule.check](value, node, ctx, rule.params));
      visit(value, isHintKey(key) ? { ...ctx, inHint: true } : ctx);
    }
  };
  const rows = Array.isArray(sc.results) ? sc.results : null;
  if (rows) {
    rows.forEach((row, i) => {
      const query = queries[row?.index ?? i] ?? queries[0] ?? {};
      if (query?.debug === true) { out.debugRows = (out.debugRows ?? 0) + 1; return; }
      const hints = allHintEntries(row).map(e => e.hint.query);
      visit(row, { inHint: false, hints, query });
    });
    const { results, ...envelope } = sc;
    visit(envelope, { inHint: false, hints: hintEntries(envelopeContainer(sc)).map(e => e.hint.query), query: queries[0] ?? {} });
  } else visit(sc, { inHint: false, hints: allHintEntries(sc).map(e => e.hint.query), query: queries[0] ?? {} });
  return out;
}
/** Every executable continuation anywhere under a node (row- or item-level containers). */
export function allHintEntries(node, out = []) {
  if (!node || typeof node !== 'object') return out;
  for (const [key, child] of Object.entries(node)) {
    if (isHintContainer(key, child)) out.push(...hintEntries(child));
    else allHintEntries(child, out);
  }
  return out;
}
export function mergeCounts(into, from) {
  for (const [k, v] of Object.entries(from ?? {})) {
    if (typeof v === 'number') into[k] = (into[k] ?? 0) + v;
    else { const r = (into[k] ??= { count: 0, bytes: 0 }); r.count += v.count; r.bytes += v.bytes; }
  }
  return into;
}

// ---------- verbatim replay ----------
/** Canonical JSON (sorted keys) for comparing queries. */
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
}
/** Call rows of an argument object: `queries[]`, or the arguments themselves (flat or raw calls). */
export const callRows = args => (Array.isArray(args?.queries) ? args.queries : [args ?? {}]);

// ---------- byte-regression gate ----------
export const GATE_FACTOR = 1.5;
export const ANCHOR_FILE = 'competitors-anchor.json';
export const ROLLING_FILE = 'competitors-rolling.json';
/** The per-task record a baseline keeps. */
export function baselineRecord(task) {
  const o = task.octocode;
  return {
    kind: task.kind, tool: task.tool,
    bytes: o.bytes, calls: o.calls, cBytes: task.competitor.bytes, cCalls: task.competitor.calls,
    octoRecipe: task.recipeHash?.octocode, shellRecipe: task.recipeHash?.shell,
    bodyHash: o.sensors?.bodyHash, keyBytes: o.sensors?.keyBytes, keyShares: o.sensors?.keyShares,
    evidence: o.evidence, violations: o.sensors?.violations?.length ?? 0, schemaErrors: o.sensors?.schemaErrors ?? 0,
  };
}
const ratioOf = (a, b) => (b > 0 ? a / b : Infinity);
/**
 * Gate one task against the pinned anchor and the rolling baseline.
 * A metric (bytes, calls) fails when it exceeds GATE_FACTOR × a baseline AND
 * its ratio to the shell worsens against that baseline (when the shell recipe
 * changed, the ratio is not comparable and the absolute growth alone fails).
 * Coverage pairing: local tasks fail when unique evidence (file:line pairs or
 * files) shrinks or never-trim violations grow, so a smaller response that
 * drops evidence never passes. A changed octocode recipe is not comparable
 * and is reported, not gated (re-anchor explicitly).
 */
export function gateTask(cur, baselines, factor = GATE_FACTOR) {
  const flags = [];
  const notes = [];
  for (const [label, base] of Object.entries(baselines)) {
    const was = base?.tasks?.[cur.id];
    if (!was) { notes.push(`${label}: no baseline`); continue; }
    if (was.octoRecipe && cur.octoRecipe && was.octoRecipe !== cur.octoRecipe) { notes.push(`${label}: octocode recipe changed; not gated`); continue; }
    const shellChanged = !!(was.shellRecipe && cur.shellRecipe && was.shellRecipe !== cur.shellRecipe);
    for (const [metric, shell] of [['bytes', 'cBytes'], ['calls', 'cCalls']]) {
      const grew = cur[metric] > factor * was[metric];
      const worse = shellChanged || ratioOf(cur[metric], cur[shell]) > ratioOf(was[metric], was[shell]);
      if (grew && worse) {
        const why = metric === 'bytes' ? keyGrowth(cur.keyBytes, was.keyBytes) : [];
        flags.push({ baseline: label, metric, was: was[metric], now: cur[metric], growth: +(cur[metric] / was[metric]).toFixed(2), ratioWas: +ratioOf(was[metric], was[shell]).toFixed(2), ratioNow: +ratioOf(cur[metric], cur[shell]).toFixed(2), shellChanged, cause: why });
      }
    }
    if (cur.kind === 'local' && was.evidence && cur.evidence) {
      for (const k of ['pairs', 'files']) if (cur.evidence[k] < was.evidence[k]) flags.push({ baseline: label, metric: `evidence.${k}`, was: was.evidence[k], now: cur.evidence[k] });
    }
    if (cur.violations > (was.violations ?? 0)) flags.push({ baseline: label, metric: 'neverTrimViolations', was: was.violations ?? 0, now: cur.violations });
  }
  return { flags, notes };
}
export const describeFlag = f => `${f.metric} ${f.was}→${f.now}${f.growth ? ` (${f.growth}×, shell ratio ${f.ratioWas}→${f.ratioNow}${f.shellChanged ? ', shell recipe changed' : ''})` : ''} vs ${f.baseline}${f.cause?.length ? `: ${f.cause.join(', ')}` : ''}`;

// ---------- self-test ----------
export function selfTest() {
  let failed = 0;
  const assert = (ok, name) => { console.log(`${ok ? 'PASS' : 'FAIL'} [sensors self-test] ${name}`); if (!ok) failed += 1; };
  const hint = (tool, query) => ({ tool, query });

  // Hints: `next` and `hints` read the same; envelope too; prose `hints` are not hints.
  const withNext = { results: [{ data: { next: { nextPage: hint('localSearch', { path: 'a', page: 2 }) } } }], responsePagination: { hasMore: true, next: hint('localSearch', { responseCharOffset: 10 }) } };
  const withHints = { results: [{ data: { hints: { nextPage: hint('localSearch', { path: 'a', page: 2 }) } } }], responsePagination: { hasMore: true, hints: hint('localSearch', { responseCharOffset: 10 }) } };
  assert(canonical(allHintEntries(withNext)) === canonical(allHintEntries(withHints)) && allHintEntries(withHints).length === 2, 'hints and next walk to the same continuations (row + envelope)');
  assert(hintEntries(envelopeContainer(withHints)).length === 1 && hintEntries(envelopeContainer(withNext)).length === 1, 'envelope continuation read from responsePagination.next (or .hints)');
  assert(allHintEntries({ results: [{ status: 'error', data: { hints: ['Verify the path exists'] } }] }).length === 0, 'error-row prose hints are not continuations');
  // Legacy streams: leads inside `next`, prose `hints`. Current contract: pages in `next`, leads + text in `hints`.
  const today = { next: { nextPage: hint('localSearch', { page: 2 }), readFixPr: hint('ghGetHistoryItem', { number: 1 }), viewRepo: hint('ghStructure', { repo: 'r' }) }, hints: ['prose'] };
  const after = { next: { nextPage: hint('localSearch', { page: 2 }) }, hints: { text: ['prose'], readFixPr: hint('ghGetHistoryItem', { number: 1 }), viewRepo: hint('ghStructure', { repo: 'r' }) } };
  assert(leadEntries(today).length === 2 && leadEntries(after).length === 2 && maxLeadEntries({ results: [{ data: after }] }) === 2, 'leads counted the same in both shapes (pages uncapped)');
  assert(pageEntries({ results: [{ data: today }] }).length === 1 && pageEntries({ results: [{ data: after }], responsePagination: { next: hint('x', { responseCharOffset: 9 }) } }).length === 2, 'page continuations read from next only (+ envelope)');
  assert(leadBytes(today) === leadBytes(after) && leadBytes(today) > 0, 'lead bytes comparable across the move');
  assert(allHintEntries(after).length === 3, 'replay set: every next entry and every non-text hints entry');
  // The page/lead rule is core's: clasify's own walk is a page, a clasify handoff elsewhere is a lead.
  const clasifyWalk = { queries: [{ next: { clasify: hint('clasify', { resources: [] }), read: hint('localFetch', { path: 'a' }) } }] };
  assert(pageEntries(clasifyWalk).length === 1 && maxLeadEntries(clasifyWalk) === 1, 'legacy clasify: next.clasify is a page, next.read a lead');
  assert(leadEntries({ next: { clasify: hint('clasify', {}), expandCaptures: hint('astSearch', {}) } }, 'localSearch').length === 1, 'legacy handoff: clasify on another tool is a lead, expand* a page');

  // Schema errors.
  assert(schemaErrors({ isError: true, raw: undefined, text: 'Input validation error: Invalid arguments for tool localFetch: queries.0.ranges.0: Use "start-end"' }).count === 1, 'call-level input validation error counted');
  assert(schemaErrors({ isError: true, raw: { results: [{ status: 'error', data: { errorCode: 'invalidPagination' } }, { status: 'error', data: { errorCode: 'pathNotFound' } }] } }).count === 1, 'row schema errorCode counted, runtime miss not');
  assert(schemaErrors({ isError: false, raw: { results: [{ data: {} }] }, text: '' }).count === 0, 'clean response has no schema error');

  // Verbose fields.
  const rules = loadVerboseRules();
  const v = verboseFields({ args: { queries: [{ path: 'a', searchText: 'x' }] }, raw: {
    results: [{ data: { warnings: [`binaryFileSkipped: ${Array.from({ length: 40 }, (_, i) => `f${i}.woff2`).join(', ')}`], pagination: { hasMore: true, nextPage: 2, snapshot: 'abc' }, next: { nextPage: hint('localSearch', { path: 'a', page: 2, snapshot: 'abc', confidence: 'x' }) } } }],
    responsePagination: { hasMore: false, charOffset: 0 },
  } }, rules);
  assert(v['responsePagination-finished']?.count === 1, 'verbose: responsePagination with hasMore:false reported');
  assert(v['pagination-duplicates-hint']?.count === 1 && v['pagination-duplicates-hint'].bytes > 0, 'verbose: pagination nextPage/snapshot duplicating the continuation reported');
  assert(v['warnings-name-list']?.count === 1, 'verbose: inlined skipped-file name list reported');
  const quiet = verboseFields({ args: { queries: [{}] }, raw: { results: [{ data: { pagination: { hasMore: true, nextPage: 2 } } }], responsePagination: { hasMore: true } } }, rules);
  assert(Object.keys(quiet).length === 0, 'verbose: needed pagination (hasMore:true, no duplicate) not reported');
  assert(verboseFields({ args: { queries: [{ debug: true }] }, raw: { results: [{ data: { responsePagination: { hasMore: false } } }] } }, rules).debugRows === 1, 'verbose: debug:true rows skipped');
  const partial = (providerLimit) => verboseFields({ args: { queries: [{}] }, raw: { results: [{ data: { isPartial: true, partialReasons: ['GitHub code search caps results at 1000'], providerLimit } }] } }, rules);
  assert(Object.keys(partial({ maxResults: 1000 })).length === 0, 'verbose: isPartial + partialReasons + providerLimit{maxResults} carry different facts (not reported)');
  assert(partial({ maxResults: 1000, reason: 'GitHub code search caps results at 1000' })['partial-reason-repeated']?.count === 1, 'verbose: providerLimit.reason repeating a partialReasons entry reported');

  // Byte gate.
  const base = (bytes, calls, cBytes = 1000, extra = {}) => ({ tasks: { T1: { kind: 'local', bytes, calls, cBytes, cCalls: 1, octoRecipe: 'r', shellRecipe: 's', evidence: { pairs: 10, files: 2 }, violations: 0, keyBytes: { 'data.files': bytes }, ...extra } } });
  const cur = (bytes, calls, extra = {}) => ({ id: 'T1', kind: 'local', bytes, calls, cBytes: 1000, cCalls: 1, octoRecipe: 'r', shellRecipe: 's', evidence: { pairs: 10, files: 2 }, violations: 0, keyBytes: { 'data.files': 2000, 'data.warnings': bytes - 2000 }, ...extra });
  const g2x = gateTask(cur(4000, 1), { anchor: base(2000, 1), rolling: base(2000, 1) });
  assert(g2x.flags.some(f => f.metric === 'bytes' && f.baseline === 'anchor'), 'gate: synthetic 2× byte growth flagged');
  assert(g2x.flags.find(f => f.metric === 'bytes')?.cause?.[0]?.startsWith('data.warnings'), 'gate: flag names the key that grew (data.warnings)');
  assert(gateTask(cur(2400, 1), { anchor: base(2000, 1), rolling: base(2000, 1) }).flags.length === 0, 'gate: 1.2× growth passes');
  assert(gateTask(cur(2000, 2), { anchor: base(2000, 1) }).flags.some(f => f.metric === 'calls'), 'gate: 2× calls flagged');
  assert(gateTask(cur(1900, 1), { anchor: base(1000, 1), rolling: base(1400, 1) }).flags.some(f => f.baseline === 'anchor' && f.metric === 'bytes'), 'gate: ratchet (1.36× rolling, 1.9× anchor) caught by the pinned anchor');
  assert(gateTask(cur(4000, 1, { cBytes: 4000 }), { anchor: base(2000, 1) }).flags.length === 0, 'gate: growth with an unchanged shell ratio passes (ratio must worsen)');
  assert(gateTask(cur(4000, 1, { cBytes: 4000, shellRecipe: 's2' }), { anchor: base(2000, 1) }).flags.length > 0, 'gate: changed shell recipe gates on absolute growth alone');
  assert(gateTask(cur(1000, 1, { evidence: { pairs: 7, files: 2 } }), { anchor: base(2000, 1) }).flags.some(f => f.metric === 'evidence.pairs'), 'gate: a smaller response that drops evidence fails');
  assert(gateTask(cur(1000, 1, { violations: 1 }), { anchor: base(2000, 1) }).flags.some(f => f.metric === 'neverTrimViolations'), 'gate: new never-trim violation fails');
  assert(gateTask(cur(9000, 1, { octoRecipe: 'r2' }), { anchor: base(2000, 1) }).notes.some(n => /recipe changed/.test(n)), 'gate: changed octocode recipe reported, not compared');

  // Key shares.
  const kb = keyBytes([{ raw: { shared: { a: 1 }, results: [{ index: 0, data: { files: [1, 2], warnings: ['x'] } }] } }]);
  assert(kb['data.files'] > 0 && kb['data.warnings'] > 0 && kb.shared > 0 && kb['results[].index'] > 0, 'key bytes split envelope, row and data keys');
  assert(Math.abs(Object.values(keyShares(kb)).reduce((a, b) => a + b, 0) - 1) < 0.01, 'key shares sum to 1');
  assert(bodyHash([{ tool: 't', raw: { a: 1 } }]) !== bodyHash([{ tool: 't', raw: { a: 2 } }]), 'body hash changes with the body');

  console.log(`\n[sensors self-test] ${failed ? `${failed} failed` : 'all passed'}`);
  return failed;
}

if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv.includes('--self-test')) process.exitCode = selfTest() ? 1 : 0;
