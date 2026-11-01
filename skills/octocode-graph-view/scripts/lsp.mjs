// Resolved call hierarchy from the language server over the stitched graph: callees of the functions in hub
// files and of every call bound between chunks by name. Each graph call it probes is labelled `both` or
// `graph`; callees only the server reports join as `lsp` edges. Results stream as `lsp` events per batch.
import { relative, resolve, join } from 'node:path';
import { octocode, pool } from './lib.mjs';
import { continuations, pagedTool } from './tools.mjs';

const fileOf = (id) => id.split('#')[0];
const lastName = (name) => String(name ?? '').split('.').pop().replace(/@\d+$/, '');
const keyOf = (file, name) => `${file}#${lastName(name)}`;
const symbolName = (id) => id.split('#')[1] ?? '';

// The server's own batch limit (`queries.maxItems` in the published schema); one query per call otherwise.
async function batchLimit() {
  try {
    const schema = await octocode(['schema', 'lspSearch', '--json'], { partial: true });
    const find = (v) => (v && typeof v === 'object' ? (v.queries?.maxItems ?? Object.values(v).map(find).find(Boolean)) : undefined);
    return Math.max(1, Number(find(schema)) || 1);
  } catch {
    return 1;
  }
}

// Callees of one probed symbol: definition file and name per match, or null when the server gave no answer.
// Until every operation states it in partialReasons, a related rerun with rustContext.buildScripts is the same signal:
// the answer is missing types that build scripts generate.
const buildScriptLead = (d) => (d.related ?? []).some((r) => r?.input?.queries?.some?.((q) => q?.rustContext?.buildScripts === true));

// One probe's answer: `{ callees }` (definition file, name and line per callee, plus the call-site lines in the probed
// function), or `{ unanswered }` with the server's own reasons (error code, partialReasons such as buildScriptsOff).
function calleesOf(rowPages, root) {
  const rows = rowPages.map((p) => p?.data ?? {});
  const reasons = [...new Set(rows.flatMap((d) => [...(d.errorCode ? [d.errorCode] : []), ...(d.partialReasons ?? []), ...(buildScriptLead(d) ? ['buildScriptsOff'] : [])]))];
  if (rows.some((d) => d.errorCode)) return { unanswered: reasons };
  if (rows.every((d) => d.kind === 'empty')) return rows.some((d) => d.isPartial) ? { unanswered: reasons.length ? reasons : ['partial'] } : { callees: new Map() };
  const out = new Map();
  for (const d of rows) for (const g of d.files ?? []) for (const m of (g.matches ?? []).filter((x) => x.symbolName)) addCallee(out, relative(root, resolve(m.path ?? g.path)), m);
  return { callees: out };
}
// One callee match merged by definition key: its call sites accumulate across pages and duplicate matches.
function addCallee(out, file, m) {
  const key = keyOf(file, m.symbolName);
  const cur = out.get(key) ?? { file, name: lastName(m.symbolName), line: m.line, sites: [] };
  for (const line of (m.sites ?? []).map((s) => s.line)) if (Number.isInteger(line) && !cur.sites.includes(line)) cur.sites.push(line);
  out.set(key, cur);
}

// One batch: the first response, then each row's own continuation chain, kept with its query.
async function probeBatch(batch, root) {
  const first = await octocode(['lspSearch', '--json', JSON.stringify({ queries: batch.map((x) => ({ operation: 'callees', path: join(root, x.file), symbolName: x.name, line: x.line })) })], { partial: true });
  return Promise.all(batch.map(async (x, i) => {
    const row = (first.results ?? []).find((r) => r.index === i) ?? first.results?.[i];
    const tail = [];
    for (const c of continuations({ results: [row] })) tail.push(...(await pagedTool(c.tool, c.input)).flatMap((p) => p.results ?? []));
    return [x, calleesOf([row, ...tail], root)];
  }));
}

function applyCallees(D, x, L, { index, stats }) {
  const update = [], added = [];
  const mine = D.calls.filter((c) => c.source === x.id);
  const seen = new Set(mine.map((c) => keyOf(fileOf(c.target), symbolName(c.target))));
  for (const c of mine) {
    const hit = L.has(keyOf(fileOf(c.target), symbolName(c.target)));
    c.via = hit ? 'both' : c.via === 'lsp' ? 'lsp' : 'graph';
    stats[hit ? 'both' : 'graphOnly']++;
    update.push({ source: c.source, target: c.target, via: c.via });
  }
  for (const [k, def] of L) {
    if (seen.has(k) || !index.files.has(def.file)) continue;
    const id = index.byKey.get(k) ?? `${def.file}#${def.name}`;
    if (id === x.id) continue;
    stats.lspOnly++;
    const lines = def.sites.toSorted((a, b) => a - b);
    const e = { source: x.id, target: id, line: lines[0] ?? null, lines, count: Math.max(1, lines.length), confidence: 'high', resolution: ['lsp-callee'], via: 'lsp' };
    D.calls.push(e);
    if (!index.symbolIds.has(id)) {
      index.symbolIds.add(id);
      D.symbols.push({ id, file: def.file, name: def.name, kind: 'symbol', line: def.line, calls: 0, exported: false });
    }
    added.push(e);
  }
  return { update, added };
}

export async function lspPhase(D, { root, emit, signal, jobs = 8 }) {
  // Every probe the server left unanswered is listed (never only counted), so it can be retried or reported.
  const stats = { lsp: 'pending', probed: 0, resolved: 0, both: 0, graphOnly: 0, lspOnly: 0, unanswered: [], scope: 'functions in hub files and sources of calls bound between chunks' };
  const hubs = new Set(D.hubFiles);
  const cross = new Set(D.calls.filter((c) => c.crossChunk).map((c) => c.source));
  const probe = D.symbols.filter((s) => s.line && ['function', 'method'].includes(s.kind) && (hubs.has(s.file) || cross.has(s.id)));
  emit({ type: 'progress', phase: 'lsp', done: 0, total: probe.length });
  if (!probe.length) return { ...stats, lsp: 'skipped: no functions to probe in hub files or between chunks' };
  try {
    // A warm-up answer proves a server exists for this language before the batch fan-out.
    await octocode(['lspSearch', '--json', JSON.stringify({ queries: [{ operation: 'documentSymbols', path: join(root, probe[0].file) }] })]);
  } catch (error) {
    return { ...stats, lsp: `skipped: the language server did not answer (${error.message.split('\n')[0].slice(0, 300)})` };
  }
  stats.lsp = 'ok';
  const index = { files: new Set(D.files.map((f) => f.id)), byKey: new Map(D.symbols.map((s) => [keyOf(s.file, symbolName(s.id)), s.id])), symbolIds: new Set(D.symbols.map((s) => s.id)) };
  const size = await batchLimit();
  const batches = [];
  for (let i = 0; i < probe.length; i += size) batches.push(probe.slice(i, i + size));
  await pool(batches, Math.max(1, Math.floor(jobs / 2)), async (batch) => {
    if (signal?.aborted) return;
    const answers = await probeBatch(batch, root).catch((error) => batch.map((x) => [x, { unanswered: [`callFailed: ${String(error.message).split('\n')[0]}`] }]));
    const update = [], added = [];
    for (const [x, answer] of answers) {
      stats.probed++;
      if (!answer.callees) { stats.unanswered.push({ id: x.id, file: x.file, symbol: x.name, line: x.line, reasons: answer.unanswered }); continue; }
      const L = answer.callees;
      stats.resolved++;
      const r = applyCallees(D, x, L, { index, stats });
      update.push(...r.update);
      added.push(...r.added);
    }
    emit({ type: 'lsp', update, added, probed: stats.probed, resolved: stats.resolved, total: probe.length, stats: { ...stats } });
    emit({ type: 'progress', phase: 'lsp', done: stats.probed, total: probe.length });
  });
  D.coverage.calls = D.calls.length;
  return { mode: 'both', ...stats };
}
