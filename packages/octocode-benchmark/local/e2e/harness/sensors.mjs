// BENCHMARK library (not a suite): response sensors for competitors/kpi; chain-check reuses its schema and hint helpers.
// Response sensors shared by the competitor harness (RFC tool-quality-efficiency):
// continuation (hint) walking, schema-error counting, the verbose-field
// report, body hash + key byte shares, and the byte-regression gate (pinned
// anchor plus rolling baseline). Pure functions only; `--self-test` runs them
// on synthetic responses.
//
import { nextHints } from './responses.mjs';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from '../../../../../skills/common/scripts/lib/entrypoint.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const size = value => value === undefined ? 0 : Buffer.byteLength(JSON.stringify(value), 'utf8');
const fieldSize = (key, value) => size(value) + Buffer.byteLength(JSON.stringify(key), 'utf8') + 1;

export const isHintKey = key => ['next', 'related', 'hints', 'responsePagination'].includes(key);
/** Only authored typed calls are executable; prose hints are never parsed. */
export const hintEntries = (container, name = 'next') => nextHints(container).map(call => ({ name: call.path.split('.').at(-1) || name, hint: call }));
export const envelopeContainer = sc => sc?.responsePagination?.next;
export const isHintContainer = (key, value) => isHintKey(key) && hintEntries(value).length > 0;

/** Largest related-call menu in one response. */
export function maxLeadEntries(node) {
  let max = 0;
  const walk = value => {
    if (!value || typeof value !== 'object') return;
    max = Math.max(max, hintEntries(value.related).filter(entry => entry.hint.kind === 'related').length);
    for (const child of Object.values(value)) walk(child);
  };
  walk(node);
  return max;
}

/** UTF-8 JSON bytes under each occurrence of a key, including that key. */
export function bytesUnderKey(value, keyName) {
  let total = 0;
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    for (const [key, child] of Object.entries(node)) {
      if (key === keyName) { total += fieldSize(key, child); continue; }
      walk(child);
    }
  };
  walk(value);
  return total;
}
export const leadBytes = value => bytesUnderKey(value, 'related');

// ---------- schema errors ----------
/** Row error codes that mean the input did not validate (as opposed to a runtime miss). */
const SCHEMA_ERROR_CODES = new Set(['invalidInput', 'invalidPagination']);
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
  const add = (key, value, wireKey = key) => { out[key] = (out[key] ?? 0) + fieldSize(wireKey, value); };
  for (const e of entries) {
    const sc = e.raw;
    if (!sc || typeof sc !== 'object') { out['(text)'] = (out['(text)'] ?? 0) + Buffer.byteLength(String(e.text ?? ''), 'utf8'); continue; }
    for (const [key, value] of Object.entries(sc)) {
      if (key !== 'results' || !Array.isArray(value)) { add(key, value); continue; }
      for (const row of value) {
        if (!row || typeof row !== 'object') continue;
        for (const [rk, rv] of Object.entries(row)) {
          if (rk === 'data' && rv && typeof rv === 'object' && !Array.isArray(rv)) for (const [dk, dv] of Object.entries(rv)) add(`data.${dk}`, dv, dk);
          else add(`results[].${rk}`, rv, rk);
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
function keyGrowth(now = {}, was = {}, top = 2) {
  return Object.keys({ ...now, ...was })
    .map(k => ({ key: k, delta: (now[k] ?? 0) - (was[k] ?? 0) }))
    .filter(x => x.delta > 0)
    .sort((a, b) => b.delta - a.delta)
    .slice(0, top)
    .map(x => `${x.key} +${x.delta} B`);
}

// ---------- verbose-field report ----------
const VERBOSE_FIELDS_FILE = path.join(HERE, 'verbose-fields.json');
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
      if (ctx.hints.some(q => q?.[qk] === value[f])) bytes += fieldSize(f, value[f]);
    }
    return bytes;
  },
  // A list entry that inlines a long non-evidence list (skipped-file names).
  longListEntry: (value, holder, ctx, { maxBytes = 300, maxNames = 5 } = {}) => (Array.isArray(value)
    ? value.filter(s => typeof s === 'string' && (Buffer.byteLength(s, 'utf8') > maxBytes || s.split(/,\s*/).length > maxNames)).reduce((a, s) => a + size(s), 0)
    : 0),
  // Present inside a continuation (duplicates data the row already shows).
  inHint: (value, holder, ctx) => (ctx.inHint ? size(value) : 0),
  // An object field whose value a sibling list already holds verbatim.
  repeatsSibling: (value, holder, ctx, { field, sibling } = {}) => {
    const repeated = value?.[field];
    const list = holder?.[sibling];
    return repeated != null && (Array.isArray(list) ? list : [list]).some(item => item === repeated) ? fieldSize(field, repeated) : 0;
  },
  // A continuation field with a fixed value.
  equalsInHint: (value, holder, ctx, { equals } = {}) => (ctx.inHint && value === equals ? size(value) + 3 : 0),
  // An object that only echoes the row's input.
  echoesQuery: (value, holder, ctx, { fields = [] } = {}) => (value && typeof value === 'object' && fields.length && fields.every(([rk, qk]) => value[rk] != null && value[rk] === ctx.query?.[qk]) ? size(value) : 0),
};
/**
 * Default-output fields from the verbose list in one response. Rows queried
 * with a non-empty `extras` are skipped (the sensor measures default output).
 * Returns {ruleId: {count, bytes}} plus `debugRows`.
 */
export function verboseFields(entry, rules) {
  const out = {};
  const sc = entry?.raw;
  if (!sc || typeof sc !== 'object') return out;
  const queries = callRows(entry.args);
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
      const hints = allHintEntries(row).flatMap(e => e.hint.input.queries ?? []);
      visit(row, { inHint: false, hints, query });
    });
    const { results, ...envelope } = sc;
    visit(envelope, { inHint: false, hints: hintEntries(envelopeContainer(sc)).flatMap(e => e.hint.input.queries ?? []), query: queries[0] ?? {} });
  } else visit(sc, { inHint: false, hints: allHintEntries(sc).flatMap(e => e.hint.input.queries ?? []), query: queries[0] ?? {} });
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
/** Fresh query rows; cursor continuations intentionally have no reconstructed query. */
export const callRows = args => args?.queries ?? [];

// ---------- byte-regression gate ----------
export const GATE_FACTOR = 1.5;
export const ANCHOR_FILE = 'competitors-anchor.json';
export const ROLLING_FILE = 'competitors-rolling.json';
/** The per-task record a baseline keeps. */
export function baselineRecord(task) {
  const o = task.octocode;
  return {
    kind: task.kind, tool: task.tool, measurement: 'utf8-v1',
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
function incomparableBaseline(current, baseline) {
  if (!baseline) return 'no baseline';
  if (current.measurement !== baseline.measurement) return 'measurement changed; not comparable';
  if (baseline.octoRecipe && current.octoRecipe && baseline.octoRecipe !== current.octoRecipe) return 'octocode recipe changed; not gated';
  return null;
}

export function gateTask(cur, baselines, factor = GATE_FACTOR) {
  const flags = [];
  const notes = [];
  const compared = [];
  for (const [label, base] of Object.entries(baselines)) {
    const was = base?.tasks?.[cur.id];
    const incompatibility = incomparableBaseline(cur, was);
    if (incompatibility) { notes.push(`${label}: ${incompatibility}`); continue; }
    compared.push(label);
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
  return { flags, notes, compared };
}
export const describeFlag = f => `${f.metric} ${f.was}→${f.now}${f.growth ? ` (${f.growth}×, shell ratio ${f.ratioWas}→${f.ratioNow}${f.shellChanged ? ', shell recipe changed' : ''})` : ''} vs ${f.baseline}${f.cause?.length ? `: ${f.cause.join(', ')}` : ''}`;

// ---------- self-test ----------
export function selfTest() {
  let failed = 0;
  const assert = (ok, name) => { console.log(`${ok ? 'PASS' : 'FAIL'} [sensors self-test] ${name}`); if (!ok) failed += 1; };
  const continuation = cursor => ({ kind: 'continue', tool: 'localFetch', input: { cursor } });
  const related = { kind: 'related', tool: 'localFetch', input: { queries: [{ path: '日本.ts' }] } };
  const response = { results: [{ data: { next: { source: continuation('source') }, related: [related], hints: ['read: localFetch {"queries":[]}'] } }], responsePagination: { hasMore: true, next: continuation('response') } };
  assert(allHintEntries(response).length === 3, 'typed row, related and response calls discovered');
  assert(hintEntries(envelopeContainer(response))[0].hint.input.cursor === 'response', 'response cursor preserved');
  assert(allHintEntries({ hints: ['read: localFetch {"queries":[]}'], next: { old: { tool: 'localFetch', query: {} } } }).length === 0, 'prose and obsolete call shapes are not executable');
  assert(maxLeadEntries(response) === 1, 'related calls counted independently of continuation pages');
  assert(leadBytes(response) === Buffer.byteLength(JSON.stringify({ related: [related] }), 'utf8') - 2, 'related bytes are UTF-8');
  assert(schemaErrors({ isError: true, text: 'Input validation error: bad input' }).count === 1, 'call input failure counted');
  assert(schemaErrors({ raw: { results: [{ status: 'error', data: { errorCode: 'invalidInput' } }, { status: 'error', data: { errorCode: 'pathNotFound' } }] } }).count === 1, 'schema and runtime errors remain distinct');
  const rules = loadVerboseRules();
  assert(verboseFields({ args: { queries: [{}] }, raw: { responsePagination: { hasMore: false } } }, rules)['responsePagination-finished']?.count === 1, 'verbose completed response marker reported');

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

// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
if (isMainModule(import.meta.url) && process.argv.includes('--self-test')) process.exitCode = selfTest() ? 1 : 0;
