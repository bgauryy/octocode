// Resolved call hierarchy from the language server over the stitched graph: callees of the functions in hub
// files and of every call bound between chunks by name. Each graph call it probes is labelled `both` or
// `graph`; callees only the server reports join as `lsp` edges. Results stream as `lsp` events per batch.
import { relative, resolve, join } from 'node:path';
import { pool } from './lib.mjs';
import { continuations, pagedTool, readResponse } from './tools.mjs';

const fileOf = (id) => id.split('#')[0];
const lastName = (name) => String(name ?? '').split('.').pop().replace(/@\d+$/, '');
const keyOf = (file, name) => `${file}#${lastName(name)}`;
const symbolName = (id) => id.split('#')[1] ?? '';

// A withheld location limits coverage. Each returned location still identifies a definition.
// For these partial answers, only reconcile a unique graph symbol at the reported file, name and line.
function calleesOf(rowPages, root, definitions) {
  if (!rowPages.length || rowPages.some((p) => !p?.data)) return { unanswered: ['missingResult'] };
  const rows = rowPages.map((p) => p.data);
  const reasons = [...new Set(rows.flatMap((d) => [...(d.errorCode ? [d.errorCode] : []), ...(d.partialReasons ?? [])]))];
  const partial = rows.some((d) => d.isPartial) || reasons.length > 0;
  if (partial && (rows.some((d) => d.errorCode || (d.isPartial && !d.partialReasons?.length)) ||
      reasons.length !== 1 || reasons[0] !== 'locationsWithheld')) {
    return { unanswered: reasons.length ? reasons : ['partial'] };
  }
  const callees = new Map();
  for (const d of rows) for (const group of d.files ?? []) for (const match of group.matches ?? []) {
    if (!match.symbolName) continue;
    const file = relative(root, resolve(match.path ?? group.path));
    const key = keyOf(file, match.symbolName);
    const id = partial ? definitions.get(key)?.get(match.line) : key;
    if (id) addCallee(callees, id, file, match);
  }
  return partial ? { callees, unanswered: reasons } : { callees };
}

// Merge call sites for one definition across pages.
function addCallee(out, key, file, m) {
  const cur = out.get(key) ?? { file, name: lastName(m.symbolName), line: m.line, sites: [] };
  for (const line of (m.sites ?? []).map((s) => s.line)) if (Number.isInteger(line) && !cur.sites.includes(line)) cur.sites.push(line);
  out.set(key, cur);
}

// One batch: the first response, then each row's own continuation chain, kept with its query.
async function probeBatch(batch, root, definitions) {
  const first = await readResponse('lspSearch', { queries: batch.map((x) => ({ operation: 'callees', path: join(root, x.file), symbolName: x.name, line: x.line })) });
  return Promise.all(batch.map(async (x, i) => {
    const rows = (first.results ?? []).filter((r) => r.index === i);
    const tail = [];
    for (const c of continuations({ results: rows })) tail.push(...(await pagedTool(c.tool, c.input)).flatMap((p) => p.results ?? []));
    return [x, calleesOf([...rows, ...tail], root, definitions)];
  }));
}

function applyCallees(D, x, answer, index, stats) {
  const update = [], added = [];
  const L = answer.callees;
  const mine = index.calls.get(x.id) ?? [];
  const seen = new Set();
  for (const c of mine) {
    const key = answer.unanswered ? c.target : keyOf(fileOf(c.target), symbolName(c.target));
    seen.add(key);
    const hit = L.has(key);
    if (!hit && answer.unanswered) continue;
    c.via = hit ? 'both' : c.via === 'lsp' ? 'lsp' : 'graph';
    stats[hit ? 'both' : 'graphOnly']++;
    update.push({ source: c.source, target: c.target, via: c.via });
  }
  for (const [k, def] of L) {
    if (seen.has(k) || !index.files.has(def.file)) continue;
    const id = answer.unanswered ? k : index.byKey.get(k) ?? `${def.file}#${def.name}`;
    if (id === x.id) continue;
    stats.lspOnly++;
    const lines = def.sites.toSorted((a, b) => a - b);
    const e = { source: x.id, target: id, line: lines[0] ?? null, lines, count: Math.max(1, lines.length), confidence: 'high', resolution: ['lsp-callee'], via: 'lsp' };
    D.calls.push(e);
    mine.push(e);
    index.calls.set(x.id, mine);
    if (!index.symbolIds.has(id)) {
      index.symbolIds.add(id);
      D.symbols.push({ id, file: def.file, name: def.name, kind: 'symbol', line: def.line, calls: 0, exported: false });
    }
    added.push(e);
  }
  return { update, added };
}

export async function lspPhase(D, { root, emit, signal, jobs = 8, batchSize = 1 }) {
  // Every probe the server left unanswered is listed (never only counted), so it can be retried or reported.
  const stats = { lsp: 'pending', probed: 0, resolved: 0, partialWithMatches: 0, both: 0, graphOnly: 0, lspOnly: 0, unanswered: [], scope: 'functions in hub files and sources of calls bound between chunks' };
  const hubs = new Set(D.hubFiles);
  const cross = new Set(D.calls.filter((c) => c.crossChunk).map((c) => c.source));
  const probe = D.symbols.filter((s) => s.line && ['function', 'method'].includes(s.kind) && (hubs.has(s.file) || cross.has(s.id)));
  emit({ type: 'progress', phase: 'lsp', done: 0, total: probe.length });
  if (!probe.length) return { ...stats, lsp: 'skipped: no functions to probe in hub files or between chunks' };
  stats.total = probe.length;
  const index = { files: new Set(D.files.map((f) => f.id)), byKey: new Map(D.symbols.map((s) => [keyOf(s.file, symbolName(s.id)), s.id])), symbolIds: new Set(D.symbols.map((s) => s.id)) };
  index.calls = Map.groupBy(D.calls, (call) => call.source);
  index.definitions = new Map();
  for (const symbol of D.symbols) {
    if (!Number.isInteger(symbol.line)) continue;
    const key = keyOf(symbol.file, symbolName(symbol.id));
    const lines = index.definitions.get(key) ?? new Map();
    lines.set(symbol.line, lines.has(symbol.line) ? null : symbol.id);
    index.definitions.set(key, lines);
  }
  const size = batchSize;
  const batches = [];
  for (let i = 0; i < probe.length; i += size) batches.push(probe.slice(i, i + size));
  let sentUnanswered = 0;
  await pool(batches, Math.max(1, Math.floor(jobs / 2)), async (batch) => {
    if (signal?.aborted) {
      for (const x of batch) stats.unanswered.push({ id: x.id, file: x.file, symbol: x.name, line: x.line, reasons: ['cancelled'] });
      return;
    }
    const answers = await probeBatch(batch, root, index.definitions).catch((error) => batch.map((x) => [x, { unanswered: [`callFailed: ${String(error.message).split('\n')[0]}`] }]));
    const update = [], added = [];
    for (const [x, answer] of answers) {
      stats.probed++;
      if (answer.unanswered) {
        stats.unanswered.push({ id: x.id, file: x.file, symbol: x.name, line: x.line, reasons: answer.unanswered });
        if (!answer.callees?.size) continue;
        stats.partialWithMatches++;
      } else {
        stats.resolved++;
      }
      const r = applyCallees(D, x, answer, index, stats);
      update.push(...r.update);
      added.push(...r.added);
    }
    const { unanswered, ...counts } = stats;
    const unansweredAdded = unanswered.slice(sentUnanswered);
    sentUnanswered = unanswered.length;
    emit({ type: 'lsp', update, added, probed: stats.probed, resolved: stats.resolved, total: probe.length, stats: counts, unansweredAdded });
    emit({ type: 'progress', phase: 'lsp', done: stats.probed, total: probe.length });
  });
  D.coverage.calls = D.calls.length;
  stats.lsp = stats.probed < probe.length ? 'cancelled' : stats.resolved ? 'ok' : stats.partialWithMatches ? 'partial' : 'unanswered';
  return { mode: 'both', ...stats };
}
