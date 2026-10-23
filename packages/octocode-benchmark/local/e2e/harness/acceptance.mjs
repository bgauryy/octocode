// Acceptance checks: pure check functions over one tool
// response (or the published surface). chain-check.mjs runs them live; the
// fixtures in acceptance.test.mjs prove each one fails a known-bad shape.
//
//   C1 chain fit        leads/pages validate against their target's input
//                       names; no packed string with >=2 coordinates (a
//                       symbols outline entry is the allowed packed form);
//                       prose hints name only published fields (or ship a lead).
//   C2 empty/error rows error rows carry errorCode + error + one recovery;
//                       empty rows carry a recovery; no call-level rejection.
//   C3 one base         no 0-based coordinate text in the published surface;
//                       no `column`/`character` 0 in an output.
//   C4 no silent omit   a positive omitted/unlisted/skipped/withheld/dropped/
//                       hidden count needs a `next` page or terminalLimit:true.
//   C5 description lint each description names a neighbor tool and one of its
//                       published fields; default/keyed/beta surfaces and
//                       per-tool additions fit the core-owned budgets.

import { MCP_SURFACE_BUDGETS } from '@octocodeai/config/mcp';
import { suggestedCall } from './lead-predicates.mjs';

export const CHECKS = ['C1', 'C2', 'C3', 'C4', 'C5'];

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
      if (k === 'hints' && Array.isArray(v)) {
        for (const text of v) {
          const call = suggestedCall(text);
          if (call) out.push({ at: `${p}.hints.${call.key}`, name: call.key, tool: call.tool, query: call.query });
        }
        continue;
      }
      if (k === 'next' && isObject(v)) {
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
 * coordinates.
 */
export function packedStrings(sc) {
  const out = [];
  const walk = (n, at, key) => {
    if (typeof n === 'string') { if (!SOURCE_TEXT_KEYS.has(key) && packedCoordinates(n)) out.push(`${at}=${JSON.stringify(n.slice(0, 80))}`); return; }
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
 * Lint: a `name:value` token in a prose hint must be a published field of
 * the emitting tool, or appear as a field of a lead query on the same row.
 */
function hintTextLint(sc, tool, published, toolNames = []) {
  const reasons = [];
  const pub = published(tool) ?? new Set();
  for (const r of rowsOf(sc)) {
    const texts = (Array.isArray(r.data?.hints) ? r.data.hints : []).filter(t => typeof t === 'string' && !suggestedCall(t));
    if (!texts.length) continue;
    const leadFields = new Set(continuations({ data: r.data }).flatMap(c => (c.query.queries ?? []).flatMap(q => Object.keys(q ?? {}))));
    for (const t of texts) {
      const named = toolNames.filter(n => new RegExp(`\\b${n}\\b`).test(t)).map(n => published(n) ?? new Set());
      for (const m of t.matchAll(/(?<![\w.])([a-z][A-Za-z0-9]*):(?=["[{\d]|true|false)/g)) {
        const name = m[1];
        if (!pub.has(name) && !leadFields.has(name) && !named.some(s => s.has(name))) reasons.push(`${r.at}.hints names unpublished \`${name}\` with no lead`);
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
    const leads = continuations({ data: d });
    const prose = (Array.isArray(d.hints) ? d.hints : []).filter(t => typeof t === 'string' && t.trim() && !suggestedCall(t));
    if (r.status === 'error') {
      if (typeof d.errorCode !== 'string' || !d.errorCode) reasons.push(`${r.at}: error row without errorCode`);
      if (typeof d.error !== 'string' || !d.error) reasons.push(`${r.at}: error row without error text`);
      if (!leads.length && !prose.length) reasons.push(`${r.at}: error row without a recovery (next or hints)`);
      if (d.retryable === false) reasons.push(`${r.at}: retryable:false emitted (only true is published)`);
    } else if (r.status === 'empty') {
      if (!leads.length && !prose.length) reasons.push(`${r.at}: empty row without a recovery (next or hints)`);
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
 * - lspSearch `orderHint`: an index into the symbol list, not a source position.
 * - clasify `questions`: a score answer is the index of its criterion in the
 *   criteria array, not a source position.
 */
const ZERO_BASED_ALLOWED = [
  /(^|\.)responseOffset\.description$/,
  /^lspSearch\.inputSchema\..*\.orderHint\.description$/,
  /^clasify\.inputSchema\..*\.questions\.description$/,
];
/** Published texts (descriptions at any depth) naming a 0-based coordinate. */
export function zeroBasedTexts(published) {
  const out = [];
  const walk = (n, at) => {
    if (typeof n === 'string') {
      const text = /^astSearch\.inputSchema\..*\.rule\.description$/.test(at)
        ? n.replace(/\brange: exact 0-based scalar bounds\b/gi, '')
        : n;
      if (ZERO_BASED.test(text) && !ZERO_BASED_ALLOWED.some(re => re.test(at))) out.push(`${at}: ${text.match(ZERO_BASED)[0]}`);
      return;
    }
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
/** Field-shaped words a note may carry besides camelCase names (core's KNOWN_FIELD_WORDS). */
const NOTE_FIELD_WORDS = new Set(['commitSha', 'headSha', 'mergeCommitSha', 'localPath', 'sourceRef', 'line', 'endLine', 'number', 'path', 'repository', 'owner', 'repo', 'ref', 'dir', 'blocks', 'symbolName', 'location']);
/**
 * One description: ends in one ` Next:` note (core's description grammar,
 * octocode-core toolDescriptions.test.ts). Core's note rule
 * (workflowNotes.test.ts): a note may route by purpose alone ("content →
 * localSearch"), but every field word in a ";" segment that names a tool must
 * be one a tool named there takes or this tool returns; a segment naming no
 * tool is prose. `publishedFields(tool)` → Set.
 */
export function descriptionLint(tool, description, toolNames, publishedFields, ownOutputs = new Set()) {
  const reasons = [];
  const note = / Next: (.+)$/.exec(String(description ?? ''))?.[1] ?? '';
  if (!note) return ['no Next: note'];
  for (const segment of note.split(';')) {
    const named = toolNames.filter(t => new RegExp(`(?<![\\w.])${t}(?![\\w])`).test(segment));
    if (!named.length) continue;
    const words = [...segment.matchAll(/[A-Za-z]\w*/g)].map(match => match[0])
      .filter(word => !toolNames.includes(word) && (NOTE_FIELD_WORDS.has(word) || /^[a-z]+[A-Z]\w*$/.test(word)));
    for (const word of words)
      if (!ownOutputs.has(word) && !named.some(t => publishedFields(t)?.has(word)))
        reasons.push(`${word}: neither ${named.join('/')} takes it nor ${tool} returns it`);
  }
  return reasons;
}
/** Check a core-measured MCP surface against its capability profile. */
export function surfaceBudget(measured, { profile = 'default', baseline } = {}) {
  const reasons = [];
  const budget = MCP_SURFACE_BUDGETS.totals[profile];
  if (budget === undefined) throw new Error(`Unknown MCP surface profile: ${profile}`);
  if (measured.total > budget) reasons.push(`surface ${measured.total} B > ${budget} B (+${measured.total - budget})`);
  const expected = new Set(Object.keys(MCP_SURFACE_BUDGETS.tools));
  if (profile === 'keyedBeta') expected.add('clasify');
  for (const name of expected) if (!(name in measured.perTool)) reasons.push(`missing tool ${name}`);
  for (const name of Object.keys(measured.perTool)) if (!expected.has(name)) reasons.push(`unexpected tool ${name}`);
  if (profile === 'default') {
    if (measured.instructions > MCP_SURFACE_BUDGETS.instructions) reasons.push(`instructions ${measured.instructions} B > quota ${MCP_SURFACE_BUDGETS.instructions} B`);
    for (const [name, quota] of Object.entries(MCP_SURFACE_BUDGETS.tools)) {
      if (measured.perTool[name] > quota) reasons.push(`${name} ${measured.perTool[name]} B > quota ${quota} B`);
    }
  } else if (!baseline) {
    reasons.push('keyed surface requires the default baseline');
  } else {
    for (const name of Object.keys(MCP_SURFACE_BUDGETS.tools)) {
      if (!(name in measured.perTool) || !(name in baseline.perTool)) continue;
      const added = measured.perTool[name] - baseline.perTool[name];
      const quota = MCP_SURFACE_BUDGETS.selectionAdditions[name];
      if (quota === undefined && added !== 0) reasons.push(`${name} unexpectedly changes by ${added} B`);
      if (quota !== undefined && added <= 0) reasons.push(`${name} is missing its keyed selection surface`);
      if (quota !== undefined && added > quota) reasons.push(`${name} adds ${added} B > quota ${quota} B`);
    }
  }
  // Core caps the evaluate/models entry at `clasify` and section research
  // separately at `sectionsAddition`; the published entry carries both.
  const clasifyQuota = MCP_SURFACE_BUDGETS.clasify + MCP_SURFACE_BUDGETS.sectionsAddition;
  if (measured.perTool.clasify > clasifyQuota) reasons.push(`clasify ${measured.perTool.clasify} B > quota ${clasifyQuota} B`);
  return { ...measured, profile, budget, reasons };
}
