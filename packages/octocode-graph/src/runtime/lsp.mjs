// Compare the stitched graph with call hierarchy from the selected declarations.
// Each batch streams confirmed, new and unanswered relationships.
import { relative, resolve, join } from 'node:path';
import { pool, nativeWorkspaceRoot } from './lib.mjs';
import { continuations, pagedTool, readResponse } from './tools.mjs';

const lastName = (name) => String(name ?? '').split('.').pop().replace(/@\d+$/, '');
const keyOf = (file, name) => `${file}#${lastName(name)}`;
// The tool distinguishes a complete empty answer (noLocations) from an unavailable one.
export function lspReasons(data) {
  return [...new Set([...(data.partialReasons ?? []), ...(data.errorCode ? [data.errorCode] : []),
    ...(data.category && data.category !== 'noLocations' ? [data.category] : []),
    ...(data.isPartial && !data.partialReasons?.length && !data.category ? ['partial'] : [])])];
}

// Each returned declaration is identified by file, name and line. A complete response can
// introduce an unindexed declaration; a withheld-location response confirms indexed positives only.
function calleesOf(rowPages, root, definitions, base) {
  if (!rowPages.length || rowPages.some((p) => !p?.data)) return { unanswered: ['missingResult'] };
  const rows = rowPages.map((p) => p.data);
  const reasons = [...new Set(rows.flatMap(lspReasons))];
  if (rows.some(d => d.resolvedSymbol?.lineDeviation)) return { unanswered: [...reasons, 'movedSource'] };
  const partial = rows.some((d) => d.isPartial) || reasons.length > 0;
  if (partial && (rows.some((d) => d.errorCode || (d.isPartial && !d.partialReasons?.length)) ||
      reasons.length !== 1 || reasons[0] !== 'locationsWithheld')) {
    return { unanswered: reasons.length ? reasons : ['partial'] };
  }
  const callees = new Map(), ambiguous = [];
  for (const d of rows) for (const group of d.files ?? []) for (const match of group.matches ?? []) {
    if (!match.symbolName || !Number.isInteger(match.line)) continue;
    const file = relative(root, resolve(base, match.path ?? group.path));
    const key = keyOf(file, match.symbolName);
    const exact = definitions.get(key)?.get(match.line);
    if (exact === null) {
      ambiguous.push({ file, name: match.symbolName, line: match.line, sites: match.sites ?? [] });
      continue;
    }
    const id = exact ?? (partial ? undefined : `${key}@${match.line}`);
    if (id) addCallee(callees, id, file, match);
  }
  if (ambiguous.length) return { callees, unanswered: [...reasons, 'ambiguousDefinition'], ambiguous };
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
  const base = await nativeWorkspaceRoot();
  const first = await readResponse('lspSearch', { queries: batch.map((x) => ({ operation: 'callees', path: join(root, x.file), symbolName: x.name, line: x.line })) });
  return Promise.all(batch.map(async (x, i) => {
    const rows = (first.results ?? []).filter((r) => r.index === i);
    const tail = [];
    for (const c of continuations({ results: rows })) tail.push(...(await pagedTool(c.tool, c.input)).flatMap((p) => p.results ?? []));
    return [x, calleesOf([...rows, ...tail], root, definitions, base)];
  }));
}

function applyCallees(D, x, answer, index, stats) {
  const update = [], added = [];
  const L = answer.callees;
  const mine = index.calls.get(x.id) ?? [];
  const seen = new Set();
  for (const c of mine) {
    seen.add(c.target);
    const hit = L.has(c.target);
    if (!hit && answer.unanswered) continue;
    c.via = hit ? 'both' : c.via === 'lsp' ? 'lsp' : 'graph';
    stats[hit ? 'both' : 'graphOnly']++;
    update.push({ source: c.source, target: c.target, via: c.via });
  }
  for (const [id, def] of L) {
    if (seen.has(id) || !index.files.has(def.file)) continue;
    if (!index.symbolIds.has(x.id)) {
      index.symbolIds.add(x.id);
      D.symbols.push({ ...x, calls: 0 });
    }
    stats.lspOnly++;
    const lines = def.sites.toSorted((a, b) => a - b);
    const e = { source: x.id, target: id, line: lines[0] ?? null, lines, count: Math.max(1, lines.length), confidence: 'high', resolution: ['lsp-callee'], via: 'lsp' };
    if (index.files.get(x.file)?.roles?.includes('test')) e.test = true;
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

export async function lspPhase(D, { root, emit, signal, declarations = [], mode = 'smart', jobs = 8, batchSize = 1 }) {
  // Every probe the server left unanswered is listed (never only counted), so it can be retried or reported.
  const stats = { lsp: 'pending', probed: 0, resolved: 0, partialWithMatches: 0, both: 0, graphOnly: 0, lspOnly: 0, unanswered: [], scope: mode === 'all' ? 'all callable declarations' : 'functions in hub files and sources of calls bound between chunks' };
  const hubs = new Set(D.hubFiles);
  const cross = new Set(D.calls.filter((c) => c.crossChunk).map((c) => c.source));
  const symbols = new Map(D.symbols.map(s => [s.id, s]));
  for (const d of declarations) symbols.set(d.id, d);
  const probe = [...symbols.values()].filter(s => Number.isInteger(s.line) && ['function', 'method', 'constructor'].includes(s.kind)
    && (mode === 'all' || hubs.has(s.file) || cross.has(s.id)));
  emit({ type: 'progress', phase: 'lsp', done: 0, total: probe.length });
  if (!probe.length) return { ...stats, lsp: `skipped: no callable declarations in scope (${stats.scope})` };
  stats.total = probe.length;
  const index = { files: new Map(D.files.map((f) => [f.id, f])), symbolIds: new Set(D.symbols.map((s) => s.id)) };
  index.calls = Map.groupBy(D.calls, (call) => call.source);
  index.definitions = new Map();
  for (const symbol of symbols.values()) {
    if (!Number.isInteger(symbol.line)) continue;
    const key = keyOf(symbol.file, symbol.name);
    const lines = index.definitions.get(key) ?? new Map();
    lines.set(symbol.line, lines.has(symbol.line) && lines.get(symbol.line) !== symbol.id ? null : symbol.id);
    index.definitions.set(key, lines);
  }
  const size = batchSize;
  const batches = [];
  const unique = probe.filter(x => {
    if (index.definitions.get(keyOf(x.file, x.name)).get(x.line) !== null) return true;
    stats.unanswered.push({ id:x.id, file:x.file, symbol:x.name, line:x.line, reasons:['ambiguousSource'] });
    return false;
  });
  const ambiguous = probe.length - unique.length;
  for (let i = 0; i < unique.length; i += size) batches.push(unique.slice(i, i + size));
  let sentUnanswered = 0;
  function publish(update, added) {
    const { unanswered, ...counts } = stats;
    const unansweredAdded = unanswered.slice(sentUnanswered);
    sentUnanswered = unanswered.length;
    emit({ type: 'lsp', update, added, probed: stats.probed, resolved: stats.resolved, total: probe.length, stats: counts, unansweredAdded });
    emit({ type: 'progress', phase: 'lsp', done: stats.probed + ambiguous, total: probe.length });
  }
  if (ambiguous) publish([], []);
  await pool(batches, Math.max(1, Math.floor(jobs / 2)), async (batch) => {
    if (signal?.aborted) {
      for (const x of batch) stats.unanswered.push({ id: x.id, file: x.file, symbol: x.name, line: x.line, reasons: ['cancelled'] });
      publish([], []);
      return;
    }
    const answers = await probeBatch(batch, root, index.definitions).catch((error) => batch.map((x) => [x, { unanswered: [`callFailed: ${String(error.message).split('\n')[0]}`] }]));
    const update = [], added = [];
    for (const [x, answer] of answers) {
      stats.probed++;
      if (answer.unanswered) {
        stats.unanswered.push({ id: x.id, file: x.file, symbol: x.name, line: x.line, reasons: answer.unanswered, ...(answer.ambiguous && { ambiguousDefinitions: answer.ambiguous }) });
        if (!answer.callees?.size) continue;
        stats.partialWithMatches++;
      } else {
        stats.resolved++;
      }
      const r = applyCallees(D, x, answer, index, stats);
      update.push(...r.update);
      added.push(...r.added);
    }
    publish(update, added);
  });
  D.coverage.calls = D.calls.length;
  if (stats.probed < unique.length) stats.lsp = 'cancelled';
  else if (!stats.unanswered.length) stats.lsp = 'ok';
  else stats.lsp = stats.resolved || stats.partialWithMatches ? 'partial' : 'unanswered';
  return { mode: 'both', ...stats };
}
