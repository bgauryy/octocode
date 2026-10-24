// BENCHMARK, not a correctness test: run-all runs it only when named.
// Cache effectiveness on real repos. lspSearch continuation pages reuse the
// server answers for an unchanged anchor (checked). Symbols pages
// re-read every file so edits are detected; their timings are reported (the
// bench runs debug builds, where they are several times slower than release).
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPOS, ROOT } from './paths.mjs';
import { checks, startServer, writeResults } from './mcp-client.mjs';
import { rowData } from './responses.mjs';

const { check, summary } = checks('perf');
const client = await startServer({ env: { OCTOCODE_BETA: '1' } });
const { call } = client;
// `lead` is a next.* lead ({tool, query}), replayed verbatim.
const timed = async (tool, query, lead) => {
  const started = performance.now();
  const out = lead ? await client.follow(lead) : await call(tool, query);
  return { out, ms: Math.round(performance.now() - started) };
};
const rows = [];

// lspSearch references of a widely used symbol (vscode URI).
{
  const file = path.join(REPOS, 'huge-ts/src/vs/base/common/uri.ts');
  const line = fs.readFileSync(file, 'utf8').split('\n').findIndex(l => /^export class URI\b/.test(l)) + 1;
  // pageSize stays below the reference count so page 2 exists.
  const first = await timed('lspSearch', { path: file, symbolName: 'URI', line: line, operation: 'references', pageSize: 20 });
  const next = rowData(first.out)?.next?.nextPage;
  const second = next ? await timed('lspSearch', null, next) : null;
  const refs = rowData(first.out)?.pagination?.totalItems;
  rows.push({ case: `lspSearch references URI (${refs} refs)`, page1: first.ms, page2: second?.ms ?? '-' });
  check('lspSearch: page 2 reuses page 1 (under 30%)', !!second && second.ms < first.ms * 0.3, `page1=${first.ms} page2=${second?.ms} refs=${refs}`);
}

// astSearch symbols over a large directory.
{
  const dir = path.join(REPOS, 'huge-ts/src/vs/editor/common');
  const first = await timed('astSearch', { operation: 'symbols', path: dir, pageSize: 100 });
  const next = rowData(first.out)?.next?.nextPage;
  const second = next ? await timed('astSearch', null, next) : null;
  rows.push({ case: `astSearch symbols (${rowData(first.out)?.pagination?.totalItems} decls)`, page1: first.ms, page2: second?.ms ?? '-' });
}

// octocode graph (CLI): ingest, an unchanged re-ingest (reused snapshot), then
// queries over the snapshot. Each call is a fresh process.
const graph = (cwd, ...argv) => {
  const started = performance.now();
  const run = spawnSync(process.execPath, [path.join(ROOT, 'packages/octocode/out/octocode.js'), 'graph', ...argv, '--json'], { cwd, encoding: 'utf8', maxBuffer: 256 << 20 });
  let json;
  try { json = JSON.parse(run.stdout); } catch {}
  return { json, status: run.status, ms: Math.round(performance.now() - started) };
};
{
  const root = path.join(REPOS, 'rust');
  const ingest = graph(root, 'ingest', '.', '--workspace', root, '--exclude', 'target');
  const reused = graph(root, 'ingest', '.', '--workspace', root, '--exclude', 'target');
  const id = ingest.json?.id ?? '';
  const dependents = graph(root, 'query', 'dependents', 'tokio/src/sync/mpsc/bounded.rs', '--depth', '6', '--graph', id, '--workspace', root);
  const cycles = graph(root, 'query', 'cycles', '--graph', id, '--workspace', root);
  rows.push({ case: 'graph tokio ingest / dependents', page1: ingest.ms, page2: dependents.ms, repeat: reused.ms, cycles: cycles.ms });
  check('graph: tokio ingest, reuse, and dependents answer', ingest.status === 0 && reused.json?.reused === true && dependents.status === 0 && (dependents.json?.total ?? 0) > 0 && cycles.status !== 5, JSON.stringify({ ingest: ingest.status, reused: reused.json?.reused, dependents: dependents.json?.total ?? dependents.json?.error, cycles: cycles.status }));
}

// astSearch match and symbols over the native crates (this repository): walk
// cost of a whole-tree pattern scan, three runs each.
{
  const crates = path.resolve(import.meta.dirname, '../../../../../packages/octocode-native/crates');
  for (const query of [
    { operation: 'match', path: crates, language: 'rust', pattern: 'fn $NAME() {}', maxFiles: 5000 },
    { operation: 'symbols', path: path.join(crates, 'engine/src'), symbolName: 'walk_builder' },
  ]) {
    const runs = [];
    for (let i = 0; i < 3; i++) runs.push(await timed('astSearch', query));
    rows.push({ case: `astSearch ${query.operation} native crates`, page1: runs[0].ms, page2: '-', repeat: Math.min(...runs.slice(1).map(r => r.ms)) });
    check(`astSearch ${query.operation} native crates succeeds`, runs.every(r => !r.out.isError && !r.out.rowErrors), JSON.stringify(rowData(runs[0].out)?.errorCode ?? ''));
  }
}

// Real-repo import-resolution tallies (manual): OCTOCODE_GRAPH_SANITY_ROOTS=a:b
// ingests each root and prints its graph diagnostic counts by code and a
// sample of unresolved internal imports.
for (const root of (process.env.OCTOCODE_GRAPH_SANITY_ROOTS ?? '').split(':').filter(Boolean)) {
  const ingest = graph(root, 'ingest', '.', '--workspace', root);
  const diagnostics = graph(root, 'query', 'diagnostics', '--graph', ingest.json?.id ?? '', '--workspace', root, '--limit', '1000');
  const rowsOf = diagnostics.json?.results ?? [];
  const byCode = {};
  for (const d of rowsOf) byCode[d.code] = (byCode[d.code] ?? 0) + 1;
  const unresolved = rowsOf.filter(d => d.code === 'unresolved-internal').slice(0, 8);
  console.log(`SANITY ${root} ms=${ingest.ms + diagnostics.ms} total=${diagnostics.json?.total ?? diagnostics.json?.error} byCode=${JSON.stringify(byCode)}\n  sample unresolved: ${JSON.stringify(unresolved, null, 2)}`);
}

console.table(rows);
const result = summary();
writeResults('perf', { ...result, rows });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
