// Per-response and per-task sensors over an octocode recipe's responses:
// continuation byte shares, menu sizes, never-trim violations (a truncation
// signal with no paging continuation and no terminal-limit disclosure), plus
// the S1 sensors from harness/sensors.mjs.
import {
  allHintEntries, bodyHash, bytesUnderKey, callRows, envelopeContainer, hintEntries, isHintContainer, isHintKey, isPageName,
  keyBytes, keyShares, leadBytes, loadVerboseRules, maxLeadEntries, mergeCounts, schemaErrors, verboseFields,
} from '../sensors.mjs';

/** Largest number of entries in any one continuation menu under a node (a row's or an item's). */
function maxNextEntries(node) {
  if (!node || typeof node !== 'object') return 0;
  let max = 0;
  for (const [key, child] of Object.entries(node)) {
    max = Math.max(max, isHintContainer(key, child) ? hintEntries(child).length : maxNextEntries(child));
  }
  return max;
}

// A paging continuation (it reaches the rest of the same evidence), as
// opposed to a lead to a different read (read, readPullRequest, viewRepo, …).
const PAGING_NAME = /page|continue|more|expand|resume|pagination|^next$/i;
const PAGING_KEYS = ['page', 'offset', 'matchPage', 'patchPage', 'responseOffset', 'after', 'resume'];
const hasPagingKey = query => PAGING_KEYS.some(k => k in query);
// A core `pages` name (e.g. nextDiagnosticPage) pages its row whatever its spelling.
const isPaging = ({ name, hint }, tool = '') => isPageName(name, tool) || PAGING_NAME.test(name) || hasPagingKey(hint.query) || (hint.query.queries ?? []).some(hasPagingKey);
// A query that asked for a window (range, match, block, view) is partial by
// request: its omission markers and isPartial flag are not truncation.
const WINDOW_KEYS = ['matchString', 'ranges', 'block', 'symbol', 'view', 'length', 'offset', 'unit', 'include', 'contextLines'];
const OMISSION = /\.\.\. \[(?:lines? \d+(?:-\d+)?|\d+ gaps in lines \d+-\d+) (?:omitted|not requested)\] \.\.\.|\[\.\.\.\s*\d+ (?:more|omitted)[^\]]*\]/;

/** The truncation signal one key/value carries (null when none). */
const MORE_FLAGS = new Set(['hasMore', 'truncated']);
const PARTIAL_FLAGS = new Set(['isPartial', 'partial']);
function trimSignal(key, value, windowed) {
  if (MORE_FLAGS.has(key) && value === true) return key;
  if (PARTIAL_FLAGS.has(key) && value === true && !windowed) return key;
  if (key === 'moreLines' && value) return key;
  if (/^(clipped|omitted)/i.test(key) && value && !(Array.isArray(value) && !value.length)) return key;
  if (typeof value === 'string' && !windowed && OMISSION.test(value)) return 'omissionMarker';
  return null;
}

/** Truncation signals under one row: [{at, signal}], skipping continuation values. */
function trimSignals(node, at, windowed, out = []) {
  if (!node || typeof node !== 'object') return out;
  if (Array.isArray(node)) { node.forEach((child, i) => trimSignals(child, `${at}[${i}]`, windowed, out)); return out; }
  for (const [key, value] of Object.entries(node)) {
    if (isHintKey(key) || key === 'responsePagination') continue;
    const here = `${at}.${key}`;
    const signal = trimSignal(key, value, windowed);
    if (signal) out.push({ at: here, signal });
    trimSignals(value, here, windowed, out);
  }
  return out;
}
const hasTerminalLimit = node => JSON.stringify(node ?? null).includes('"terminalLimit":');

/** Menu sizes, continuation names and never-trim violations of one row. */
function rowSensors(out, entry, row, i) {
  const queries = callRows(entry.args);
  out.nextEntriesMax = Math.max(out.nextEntriesMax, maxNextEntries(row));
  out.leadEntriesMax = Math.max(out.leadEntriesMax, maxLeadEntries(row, entry.tool));
  const inRow = allHintEntries(row);
  out.nextNames.push(...inRow.map(e => e.name));
  const query = queries[row?.index ?? i] ?? queries[0] ?? {};
  const windowed = WINDOW_KEYS.some(k => k in query);
  if (inRow.some(e => isPaging(e, entry.tool)) || hasTerminalLimit(row)) return;
  for (const s of trimSignals(row, `results[${i}]`, windowed)) out.violations.push({ tool: entry.tool, ...s, query: JSON.stringify(query).slice(0, 160) });
}

/**
 * Per-response sensors: nextShare, next entries per menu (max), and never-trim
 * violations (a truncation signal with no executable paging continuation in
 * its row and no terminal-limit disclosure; a split row part needs the
 * envelope continuation).
 */
function responseSensors(entry) {
  // nextEntriesMax counts the largest single `next` menu (row- or item-level).
  const sc = entry.raw;
  const out = { bytes: entry.bytes, nextBytes: 0, hintsBytes: 0, leadBytes: 0, nextEntriesMax: 0, leadEntriesMax: 0, nextNames: [], violations: [] };
  if (!sc || typeof sc !== 'object') return out;
  out.nextBytes = bytesUnderKey(sc, 'next');
  out.hintsBytes = bytesUnderKey(sc, 'hints');
  out.leadBytes = leadBytes(sc, entry.tool);
  const envelopeNext = hintEntries(envelopeContainer(sc));
  // Tool rows; clasify reports per-query rows, else the whole response is one row.
  const rows = Array.isArray(sc.results) ? sc.results : Array.isArray(sc.queries) ? sc.queries : [sc];
  rows.forEach((row, i) => {
    rowSensors(out, entry, row, i);
    if (row?.rowPart && row.rowPart.part < row.rowPart.of && !envelopeNext.length) out.violations.push({ tool: entry.tool, at: `results[${i}].rowPart`, signal: 'rowPart' });
  });
  if (sc.responsePagination?.hasMore && !envelopeNext.length && !hasTerminalLimit(sc.responsePagination)) out.violations.push({ tool: entry.tool, at: 'responsePagination.hasMore', signal: 'hasMore' });
  out.nextNames = [...new Set(out.nextNames)];
  return out;
}

let verboseRules;
/** Sensors over a recipe's responses: schema errors, verbose fields, body hash and key byte shares. */
function s1Sensors(entries) {
  verboseRules ??= loadVerboseRules();
  const schema = entries.map(schemaErrors);
  const verbose = {};
  for (const e of entries) mergeCounts(verbose, verboseFields(e, verboseRules));
  const kb = keyBytes(entries);
  return {
    schemaErrors: schema.reduce((a, x) => a + x.count, 0),
    schemaErrorDetails: schema.filter(x => x.count).map(x => `${x.codes.join(',')}: ${x.detail}`.slice(0, 200)),
    verbose, verboseBytes: Object.entries(verbose).reduce((a, [k, v]) => a + (k === 'debugRows' ? 0 : v.bytes), 0),
    bodyHash: bodyHash(entries), keyBytes: kb, keyShares: keyShares(kb),
  };
}

/** Task-level sensors over every octocode response of a recipe. */
export function taskSensors(entries, evidenceSteps) {
  const per = entries.map(responseSensors);
  const sum = key => per.reduce((a, s) => a + s[key], 0);
  const bytes = sum('bytes');
  const share = n => (bytes ? +(n / bytes).toFixed(3) : 0);
  const walks = evidenceSteps.filter(s => s.walk);
  return {
    nextBytes: sum('nextBytes'), nextShare: share(sum('nextBytes')), hintsBytes: sum('hintsBytes'), hintsShare: share(sum('hintsBytes')),
    leadBytes: sum('leadBytes'), leadShare: share(sum('leadBytes')),
    nextEntriesMax: Math.max(0, ...per.map(s => s.nextEntriesMax)),
    leadEntriesMax: Math.max(0, ...per.map(s => s.leadEntriesMax)),
    nextNames: [...new Set(per.flatMap(s => s.nextNames))],
    violations: per.flatMap(s => s.violations),
    // The recipe followed every continuation it walks to the end.
    followedAll: walks.length > 0 && walks.every(s => s.exhausted),
    ...s1Sensors(entries),
  };
}

/** One recipe step's byte report. */
export const stepBytes = e => ({ tool: e.tool, surface: e.surface, ms: e.ms, bytes: e.bytes, nextBytes: bytesUnderKey(e.raw, 'next'), hintsBytes: bytesUnderKey(e.raw, 'hints'), isError: e.isError });
