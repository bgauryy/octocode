#!/usr/bin/env node
// Scores `search` against the labelled question set (octocode-benchmark/questions/graph-search): recall@1, recall@10
// and MRR (reciprocal rank of the first right row among the shown 20), per kind and per graph, with one row per
// question. Each question runs as `search <query> <flags>` on the graph it names, the way an agent types it.
//
//   node scripts/search-eval.mjs --graph <name>=<result-dir|data.json> ... [--flags "<search flags>"]
//     [--engine <query-api.ts>] [--set <search-eval.json>] [--label <name>] [--save <file.json>] [--json]
//
// --engine scores another copy of the query engine (a saved copy of today's src/runtime/query-api.ts measures the
// baseline); a search-sidecar.ts beside it (when present) attaches the search index as the CLI does. Questions with
// `typed` also run as their signature filters when the graph's symbols carry params or returns (F12).
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { gunzipSync } from 'node:zlib';

const here = dirname(new URL(import.meta.url).pathname);
const { values } = parseArgs({
  options: {
    graph: { type: 'string', multiple: true, default: [] },
    flags: { type: 'string', default: '' },
    engine: {
      type: 'string',
      default: join(here, '../src/runtime/query-api.ts'),
    },
    set: {
      type: 'string',
      default: join(
        here,
        '../../octocode-benchmark/questions/graph-search/search-eval.json'
      ),
    },
    label: { type: 'string', default: 'search' },
    save: { type: 'string' },
    json: { type: 'boolean', default: false },
  },
});

const set = JSON.parse(readFileSync(values.set, 'utf8'));
const graphs = new Map(
  values.graph.map(g => {
    const at = g.indexOf('=');
    if (at < 1) throw new Error(`--graph takes <name>=<dir>, not ${g}`);
    const path = resolve(g.slice(at + 1));
    return [
      g.slice(0, at),
      existsSync(join(path, 'data.json')) ? join(path, 'data.json') : path,
    ];
  })
);
const missing = Object.keys(set.graphs).filter(name => !graphs.has(name));
if (missing.length)
  throw new Error(`no --graph for ${missing.join(', ')} (the set names them)`);

const engine = resolve(values.engine);
const { runQuery } = await import(pathToFileURL(engine).href);
const sidecar = join(dirname(engine), 'search-sidecar.ts');
const attach = existsSync(sidecar)
  ? (await import(pathToFileURL(sidecar).href)).withSearchIndex
  : null;
// --flags --provider: the configured classification provider reranks each answer, as the CLI does (billed requests).
const provider = join(dirname(engine), 'search-provider.ts');
const rerank =
  /(^|\s)--provider(\s|$)/.test(values.flags) && existsSync(provider)
    ? (await import(pathToFileURL(provider).href)).withProvider
    : r => r;

const SHOWN = 20;
// A row answers when its symbol is an expected one; an overload's `@N` suffix names the same declaration.
const symbolOf = row =>
  typeof row.symbol === 'string' ? row.symbol.replace(/@\d+$/, '') : null;
const rankOf = (rows, expect) => {
  const want = new Set(expect);
  const at = rows.findIndex(r => want.has(symbolOf(r)) || want.has(r.id));
  return at < 0 ? null : at + 1;
};
const median = xs => {
  const s = xs.toSorted((a, b) => a - b);
  return s.length ? s[Math.floor(s.length / 2)] : null;
};

// Every declaration the build's chunks hold (chunks/<id>/data.json.gz), with root-relative ids.
function declarations(D, dir) {
  return (D.chunks ?? []).flatMap(c => {
    const file = join(dir, 'chunks', c.id, 'data.json.gz');
    if (!existsSync(file)) return [];
    const prefix = c.path === '.' ? '' : `${c.path}/`;
    const section = JSON.parse(gunzipSync(readFileSync(file)).toString());
    return (section.declarations ?? []).map(d => ({ id: prefix + d.id }));
  });
}

const loaded = new Map();
function graphData(name) {
  let g = loaded.get(name);
  if (g) return g;
  const path = graphs.get(name);
  const t0 = performance.now();
  const D = JSON.parse(readFileSync(path, 'utf8'));
  const loadMs = performance.now() - t0;
  if (attach) attach(D, path);
  // Every expected symbol must still be in the graph (data.json's call-graph symbols or a chunk's declarations): a
  // renamed or moved one is a stale question, not a miss.
  const ids = new Set(
    [...(D.symbols ?? []), ...declarations(D, dirname(path))].map(s =>
      s.id.replace(/@\d+$/, '')
    )
  );
  const signatures = (D.symbols ?? []).some(
    s => s.params != null || s.returns != null
  );
  g = { D, ids, signatures, loadMs, first: null, times: [] };
  loaded.set(name, g);
  return g;
}

async function ask(g, text) {
  const t0 = performance.now();
  const r = await rerank(runQuery(g.D, text));
  const ms = performance.now() - t0;
  if (g.first === null) g.first = ms;
  else g.times.push(ms);
  if (!r.ok && !/nothing matches/.test(r.summary ?? ''))
    return { rows: [], error: r.error ?? r.summary, ms };
  return { rows: (r.rows ?? []).slice(0, SHOWN), ms, signals: r.stats };
}

const rows = [];
for (const q of set.questions) {
  const g = graphData(q.graph);
  const stale = q.expect.filter(id => !g.ids.has(id));
  if (stale.length)
    throw new Error(
      `${q.id}: ${stale.join(', ')} not in the ${q.graph} graph (rebuild it as the set says, or fix the question)`
    );
  const quoted = `"${q.query.replace(/["\\]/g, '\\$&')}"`;
  const answer = await ask(g, `search ${quoted} ${values.flags}`.trim());
  const rank = rankOf(answer.rows, q.expect);
  const row = {
    id: q.id,
    graph: q.graph,
    kind: q.kind,
    query: q.query,
    rank,
    top: answer.rows.slice(0, 3).map(r => symbolOf(r) ?? r.id),
    ...(answer.error && { error: answer.error }),
    ...(rank &&
      answer.rows[rank - 1].found && {
        found: answer.rows[rank - 1].found,
      }),
  };
  if (q.typed && g.signatures) {
    const typed = await ask(g, `search ${q.typed} ${values.flags}`.trim());
    row.typedRank = rankOf(typed.rows, q.expect);
  }
  rows.push(row);
}

function score(list) {
  const n = list.length;
  const at = k => list.filter(r => r.rank && r.rank <= k).length / n;
  const mrr = list.reduce((s, r) => s + (r.rank ? 1 / r.rank : 0), 0) / n;
  const round = x => Number((x * 100).toFixed(1));
  return {
    questions: n,
    recallAt1: round(at(1)),
    recallAt10: round(at(10)),
    mrr: round(mrr),
  };
}
const groupBy = key =>
  Object.fromEntries(
    Object.entries(Object.groupBy(rows, r => r[key])).map(([k, list]) => [
      k,
      score(list),
    ])
  );
const typed = rows.filter(r => 'typedRank' in r);
const result = {
  label: values.label,
  flags: values.flags,
  set: set.name,
  overall: score(rows),
  byKind: groupBy('kind'),
  byGraph: groupBy('graph'),
  ...(typed.length && {
    typed: score(typed.map(r => ({ ...r, rank: r.typedRank }))),
  }),
  timing: Object.fromEntries(
    [...loaded].map(([name, g]) => [
      name,
      {
        symbols: g.D.symbols?.length ?? 0,
        loadMs: Math.round(g.loadMs),
        firstQueryMs: Math.round(g.first ?? 0),
        medianQueryMs: Number((median(g.times) ?? 0).toFixed(1)),
      },
    ])
  ),
  rows,
};

if (values.save)
  writeFileSync(values.save, `${JSON.stringify(result, null, 2)}\n`);
if (values.json) process.stdout.write(`${JSON.stringify(result)}\n`);
else {
  const line = (name, s) =>
    `${name.padEnd(22)} ${String(s.questions).padStart(3)}  R@1 ${String(s.recallAt1).padStart(5)}  R@10 ${String(s.recallAt10).padStart(5)}  MRR ${String(s.mrr).padStart(5)}`;
  const out = [
    `${result.label}${values.flags ? ` (${values.flags})` : ''}`,
    line('overall', result.overall),
    ...Object.entries(result.byKind).map(([k, s]) => line(`kind ${k}`, s)),
    ...Object.entries(result.byGraph).map(([k, s]) => line(`graph ${k}`, s)),
    ...(result.typed ? [line('typed (F12 filters)', result.typed)] : []),
    ...Object.entries(result.timing).map(
      ([k, t]) =>
        `time ${k}: ${t.symbols} symbols, load ${t.loadMs} ms, first query ${t.firstQueryMs} ms, median ${t.medianQueryMs} ms`
    ),
    ...rows.map(
      r =>
        `${r.id.padEnd(6)} ${r.kind.padEnd(7)} ${r.rank == null ? '  -' : String(r.rank).padStart(3)}${'typedRank' in r ? ` typed ${r.typedRank ?? '-'}` : ''}  ${r.query}  → ${r.top.join(', ') || r.error || 'nothing'}`
    ),
  ];
  process.stdout.write(`${out.join('\n')}\n`);
}
