// Acceptance checks: pure check functions over one tool
// response (or the published surface). chain-check.mjs runs them live; the
// fixtures in acceptance.test.mjs prove each one fails a known-bad shape.
//
//   C1 chain fit        leads/pages validate against their target's input
//                       names; no packed string with >=2 coordinates (a
//                       symbols outline entry, P1, is the allowed packed form);
//                       prose hints name only published fields (or ship a lead).
//   C2 empty/error rows error rows carry errorCode + error + one recovery;
//                       empty rows carry a recovery; no call-level rejection.
//   C3 one base         no 0-based coordinate text in the published surface;
//                       no `column`/`character` 0 in an output.
//   C4 no silent omit   a positive omitted/unlisted/skipped/withheld/dropped/
//                       hidden count needs a `next` page or terminalLimit:true.
//   C5 description lint each description names a neighbor tool and one of its
//                       published fields; keyless surface total <= budget;
//                       the keyed clasify addition <= its own budget; quotas.

import { parseOutlineEntry } from './mcp-client.mjs';

export const CHECKS = ['C1', 'C2', 'C3', 'C4', 'C5'];
/** Arrays whose string items are symbols outline entries (P1, docs/TOOL_DATA_CONTRACT.md location rows). */
const OUTLINE_KEYS = new Set(['symbols', 'members']);
/** Mirrors core publishedSurfaceBudget.test.ts MAX_DEFAULT_SURFACE_BYTES (one prompt for every surface, 2026-10-07). */
export const SURFACE_BUDGET = 15_000;
/**
 * Bytes clasify adds when a classification key registers it: its tool only (the prompt does not change).
 * Core MAX_CLASIFY_TOOL_BYTES (2,070 since 2026-10-07: three flows + handoffs) plus 200 B headroom.
 */
export const CLASIFY_SURFACE_BUDGET = 2_270;

const SOURCE_TEXT_KEYS = new Set(['value', 'content', 'text', 'body', 'patch', 'diff', 'snippet', 'code', 'source', 'line', 'lines', 'matchString', 'message', 'title', 'description', 'error', 'warnings', 'question', 'answer', 'evidence', 'reason', 'summary', 'readme']);
const isObject = v => !!v && typeof v === 'object' && !Array.isArray(v);
const isLead = v => isObject(v) && typeof v.tool === 'string' && isObject(v.query);

/** Every executable continuation under `node` with its dotted path: [{at, name, tool, query}]. */
export function continuations(node, at = '') {
  const out = [];
  const walk = (n, p) => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) { n.forEach((c, i) => walk(c, `${p}[${i}]`)); return; }
    for (const [k, v] of Object.entries(n)) {
      if ((k === 'next' || k === 'hints') && isObject(v)) {
        if (isLead(v)) out.push({ at: `${p}.${k}`, name: k, tool: v.tool, query: v.query });
        else for (const [name, h] of Object.entries(v)) if (isLead(h)) out.push({ at: `${p}.${k}.${name}`, name, tool: h.tool, query: h.query });
        continue;
      }
      walk(v, `${p}.${k}`);
    }
  };
  walk(node, at);
  return out;
}

/** Row views of a structured response: [{at, data, status}]. */
export function rowsOf(sc) {
  if (!isObject(sc)) return [];
  const rows = Array.isArray(sc.results) ? sc.results : Array.isArray(sc.queries) ? sc.queries : [];
  return rows.map((row, i) => ({ at: `results[${i}]`, data: isObject(row?.data) ? row.data : row ?? {}, status: row?.status }));
}

// ---------- C1 chain fit ----------
/**
 * A packed location: a string carrying >=2 coordinates. The one allowed
 * packed form is `"<line>\t<value>"` (a single coordinate before a tab).
 */
export function packedCoordinates(s) {
  if (typeof s !== 'string' || s.length > 400) return 0;
  if (/^\d+\t/.test(s)) return 0;
  const head = s.split('\t')[0];
  let n = 0;
  if (/^\d{4}-\d{2}-\d{2}/.test(head)) return 0; // a date or timestamp
  n += [...head.matchAll(/(?<![\w.])\d+:\d+(?![\w.])/g)].length * 2; // L:C
  n += [...head.matchAll(/(?<![\w.:-])\d+-\d+(?![\w.:-])/g)].length * 2; // S-E (not a date)
  n += [...head.matchAll(/[A-Za-z_]\w*@\d+\b/g)].length; // name@L
  n += [...head.matchAll(/\bcol \d+\b/g)].length;
  return n >= 2 ? n : 0;
}

/**
 * Strings outside continuations and source-text keys that pack >=2
 * coordinates. A symbols outline entry under `symbols`/`members` is the
 * contract's labelled packed form and passes.
 */
export function packedStrings(sc) {
  const out = [];
  const walk = (n, at, key) => {
    if (typeof n === 'string') { if (!SOURCE_TEXT_KEYS.has(key) && !(OUTLINE_KEYS.has(key) && parseOutlineEntry(n)) && packedCoordinates(n)) out.push(`${at}=${JSON.stringify(n.slice(0, 80))}`); return; }
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) { n.forEach((c, i) => walk(c, `${at}[${i}]`, key)); return; }
    for (const [k, v] of Object.entries(n)) {
      if (k === 'next' || k === 'hints' || k === 'query' || k === 'shared') continue;
      walk(v, `${at}.${k}`, k);
    }
  };
  walk(sc, '', '');
  return out;
}

/** Top-level row property names of a query schema (union over oneOf/anyOf/allOf). */
export function rowFieldNames(querySchema) {
  const names = new Set();
  const visit = s => {
    if (!isObject(s)) return;
    for (const k of Object.keys(s.properties ?? {})) names.add(k);
    for (const b of [...(s.oneOf ?? []), ...(s.anyOf ?? []), ...(s.allOf ?? [])]) visit(b);
  };
  visit(querySchema);
  return names;
}

/**
 * C1 on one response. `inputNames(tool)` → Set of the target tool's full
 * contract row field names (published + continuation fields), or null when
 * unknown. `published(tool)` → Set of published row field names.
 */
export function checkChainFit(sc, tool, { inputNames, published, toolNames = [] } = {}) {
  const reasons = [];
  for (const c of continuations(sc)) {
    const rows = Array.isArray(c.query.queries) ? c.query.queries : null;
    if (!rows) { reasons.push(`${c.at}: lead query is not the {queries:[row]} envelope`); continue; }
    const names = inputNames?.(c.tool);
    if (!names) continue;
    for (const row of rows) for (const k of Object.keys(row ?? {})) if (!names.has(k)) reasons.push(`${c.at}→${c.tool}: field \`${k}\` is not a ${c.tool} input`);
  }
  for (const p of packedStrings(sc)) reasons.push(`packed coordinates ${p}`);
  if (published) reasons.push(...hintTextLint(sc, tool, published, toolNames));
  return reasons;
}

/**
 * X12 lint: a `name:value` token in `hints.text` must be a published field of
 * the emitting tool, or appear as a field of a lead query on the same row.
 */
export function hintTextLint(sc, tool, published, toolNames = []) {
  const reasons = [];
  const pub = published(tool) ?? new Set();
  for (const r of rowsOf(sc)) {
    const texts = [].concat(r.data?.hints?.text ?? []).filter(t => typeof t === 'string');
    if (!texts.length) continue;
    const leadFields = new Set(continuations({ data: r.data }).flatMap(c => (c.query.queries ?? []).flatMap(q => Object.keys(q ?? {}))));
    for (const t of texts) {
      const named = toolNames.filter(n => new RegExp(`\\b${n}\\b`).test(t)).map(n => published(n) ?? new Set());
      for (const m of t.matchAll(/(?<![\w.])([a-z][A-Za-z0-9]*):(?=["[{\d]|true|false)/g)) {
        const name = m[1];
        if (!pub.has(name) && !leadFields.has(name) && !named.some(s => s.has(name))) reasons.push(`${r.at}.hints.text names unpublished \`${name}\` with no lead`);
      }
    }
  }
  return reasons;
}

// ---------- C2 empty/error rows ----------
/** C2 on one probe response. `expect` = 'empty' | 'error'. `schemaErr` = sensors.schemaErrors(entry). */
export function checkRowContract(sc, expect, { schemaErr, isError, text } = {}) {
  const reasons = [];
  if (schemaErr?.count) reasons.push(`schema error ${schemaErr.codes.join(',')}: ${String(schemaErr.detail ?? '').slice(0, 120)}`);
  const rows = rowsOf(sc);
  if (!rows.length) { reasons.push(`no result rows${isError ? ` (call error: ${String(text ?? '').slice(0, 120)})` : ''}`); return reasons; }
  for (const r of rows) {
    const d = r.data ?? {};
    const leads = continuations({ data: d }).filter(c => !c.at.includes('.next.'));
    const prose = [].concat(d.hints?.text ?? []).filter(t => typeof t === 'string');
    if (r.status === 'error') {
      if (typeof d.errorCode !== 'string' || !d.errorCode) reasons.push(`${r.at}: error row without errorCode`);
      if (typeof d.error !== 'string' || !d.error) reasons.push(`${r.at}: error row without error text`);
      if (!leads.length && !prose.length) reasons.push(`${r.at}: error row without a recovery (lead or hints.text)`);
      if (d.retryable === false) reasons.push(`${r.at}: retryable:false emitted (only true is published)`);
    } else if (r.status === 'empty') {
      if (!leads.length && !prose.length) reasons.push(`${r.at}: empty row without a recovery (lead or hints.text)`);
    } else if (expect) {
      reasons.push(`${r.at}: probe expected status ${expect}, got ${r.status ?? 'ok'}`);
    }
  }
  return reasons;
}

// ---------- C3 one base ----------
const ZERO_BASED = /\b(0-based|zero-based|0-indexed|zero-indexed)\b/i;
/**
 * 0-based texts kept by decision: they are offsets or
 * an upstream grammar, not source coordinates.
 * - `responseOffset`: a character offset into the rendered response (or,
 *   with responseScope rows, a page index) — an offset, not a line/column.
 * - astRewrite `transform.substring.startChar/endChar`: ast-grep's own
 *   `substring` slice semantics (Python-style, 0-based, end exclusive);
 *   renumbering would silently shift every pasted ast-grep rule.
 */
export const ZERO_BASED_ALLOWED = [
  /(^|\.)responseOffset\.description$/,
  /^astRewrite\.inputSchema\..*\.substring\.properties\.(startChar|endChar)\.description$/,
];
/** Published texts (descriptions at any depth) naming a 0-based coordinate. */
export function zeroBasedTexts(published) {
  const out = [];
  const walk = (n, at) => {
    if (typeof n === 'string') { if (ZERO_BASED.test(n) && !ZERO_BASED_ALLOWED.some(re => re.test(at))) out.push(`${at}: ${n.match(ZERO_BASED)[0]}`); return; }
    if (!n || typeof n !== 'object') return;
    for (const [k, v] of Object.entries(n)) walk(v, at ? `${at}.${k}` : k);
  };
  walk(published, '');
  return out;
}
/** Output coordinates that can only be 0-based: `column`/`character`/`startColumn` = 0. */
export function zeroCoordinates(sc) {
  const out = [];
  const walk = (n, at) => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) { n.forEach((c, i) => walk(c, `${at}[${i}]`)); return; }
    for (const [k, v] of Object.entries(n)) {
      if (k === 'hints' || k === 'next' || k === 'query') continue;
      if (/^(column|character|startColumn|startCharacter)$/.test(k) && v === 0) out.push(`${at}.${k}=0`);
      else walk(v, `${at}.${k}`);
    }
  };
  walk(sc, '');
  return out;
}

// ---------- C4 no silent omission ----------
const OMIT = /omitted|unlisted|skipped|withheld|dropped|hidden|truncated/i;
/** Positive omission counts without a `next` page on the same object or row, or terminalLimit:true. */
export function silentOmissions(sc) {
  const out = [];
  const hasNext = o => isObject(o?.next) && Object.keys(o.next).length > 0;
  for (const r of rowsOf(sc)) {
    const rowNext = hasNext(r.data) || r.data?.terminalLimit === true;
    const walk = (n, at, parentOk) => {
      if (!n || typeof n !== 'object') return;
      if (Array.isArray(n)) { n.forEach((c, i) => walk(c, `${at}[${i}]`, parentOk)); return; }
      const ok = parentOk || hasNext(n) || n.terminalLimit === true;
      for (const [k, v] of Object.entries(n)) {
        if (k === 'hints' || k === 'next' || k === 'query') continue;
        const count = typeof v === 'number' ? v : Array.isArray(v) && OMIT.test(k) ? v.length : null;
        if (OMIT.test(k) && count && count > 0 && !ok) out.push(`${r.at}${at}.${k}=${count}`);
        walk(v, `${at}.${k}`, ok);
      }
    };
    walk(r.data, '.data', rowNext);
  }
  return out;
}

// ---------- C5 description lint ----------
/**
 * One description: names a neighbor tool, and one of that neighbor's
 * published fields. `publishedFields(tool)` → Set.
 */
export function descriptionLint(tool, description, toolNames, publishedFields) {
  const reasons = [];
  const desc = String(description ?? '');
  const neighbors = toolNames.filter(t => t !== tool && new RegExp(`\\b${t}\\b`).test(desc));
  if (!neighbors.length) reasons.push('names no next tool');
  else if (!neighbors.some(n => [...(publishedFields(n) ?? [])].some(f => new RegExp(`(?<![\\w.])${f}(?![\\w])`).test(desc)))) reasons.push(`names ${neighbors.join(',')} but none of its fields`);
  return reasons;
}
/** Surface budget: total and per-tool quotas ({tool: bytes}). */
export function surfaceBudget(perToolBytes, instructionsBytes, { budget = SURFACE_BUDGET, quotas = {} } = {}) {
  const reasons = [];
  const total = Object.values(perToolBytes).reduce((a, b) => a + (b ?? 0), 0) + instructionsBytes;
  if (total > budget) reasons.push(`surface ${total} B > ${budget} B (+${total - budget})`);
  for (const [t, q] of Object.entries(quotas)) if ((perToolBytes[t] ?? 0) > q) reasons.push(`${t} ${perToolBytes[t]} B > quota ${q} B`);
  return { total, reasons };
}
