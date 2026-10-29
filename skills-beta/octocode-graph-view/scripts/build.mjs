#!/usr/bin/env node
// Section of a repo -> data.json: files, functions, imports, calls and graph insights.
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { SKIPPED_DIRS, flags, octocode, pool, sourceSize } from './lib.mjs';

const USAGE = `usage: build.mjs <section-dir> [--out <dir>] [--max-files 30] [--hub-files 8] [--symbols 10] [--probe 16] [--jobs 8] [--calls graph|both] [--lsp-timeout 120] [--exclude <dir>]
  Ingests the section with \`octocode graph\` (a frozen copy when the tree changes mid-ingest),
  keeps the busiest connected files, expands the top hub files into functions and calls, and
  writes <out>/data.json.`;

const t0 = Date.now();
const argv = process.argv.slice(2);
const f = flags(argv, { out: '.octocode/octocode-graph-view/tmp', 'max-files': 30, 'hub-files': 8, symbols: 10, probe: 16, jobs: 8, calls: 'graph', 'lsp-timeout': 120, exclude: '' });
const section = argv[0];
if (!section || section.startsWith('-')) {
  console.error(USAGE);
  process.exit(2);
}
const out = resolve(f.out);
const maxFiles = Number(f['max-files']);
const hubFiles = Number(f['hub-files']);
const perFile = Number(f.symbols);
const probe = Number(f.probe);
const jobs = Math.max(1, Number(f.jobs));
const lspTimeout = Number(f['lsp-timeout']) * 1000;

// ingest refuses a tree that changes while it runs; the frozen copy lives under an allowed root.
const octocodeHome = join(homedir(), '.octocode');
function freeze(dir) {
  const dst = join(octocodeHome, 'tmp', 'materialize', 'v3', `${basename(resolve(dir))}-frozen`);
  mkdirSync(dst, { recursive: true });
  const skip = SKIPPED_DIRS.flatMap((d) => ['--exclude', d]);
  execFileSync('rsync', ['-a', '--delete', '--max-size=1m', ...skip, `${resolve(dir)}/`, `${dst}/`]);
  return dst;
}
const ingest = (dir) => octocode(['graph', 'ingest', dir, ...f.exclude.split(',').filter(Boolean).flatMap((e) => ['--exclude', e])]);
let snap;
let frozen = false;
const tIngest = Date.now();
try {
  snap = await ingest(section);
} catch (e) {
  if (!/changed during graph analysis/.test(e.message)) throw e;
  frozen = true;
  snap = await ingest(freeze(section));
}
const ingestMs = Date.now() - tIngest;
const q = (op, ...rest) => octocode(['graph', 'query', op, ...rest, '--graph', snap.id, '--json']);

// Rank files by import degree in both directions, then keep the connected top.
const [byIn, byOut] = await Promise.all([q('hubs', '--limit', '1000'), q('hubs', '--direction', 'out', '--limit', '1000')]);
const degree = new Map();
for (const n of [...byIn.results, ...byOut.results]) if (n.kind === 'file') degree.set(n.id, n);
const ranked = [...degree.values()].sort((a, b) => b.fanIn + b.fanOut - (a.fanIn + a.fanOut)).slice(0, maxFiles);
const ids = new Set(ranked.map((n) => n.id));

const imports = [];
const externals = new Map();
await pool(ranked, jobs, async (n) => {
  const r = await q('deps', n.id, '--edge', 'imports', '--limit', '1000');
  for (const e of r.results) {
    if (ids.has(e.id)) imports.push({ source: n.id, target: e.id, line: e.edgeLine });
    else if (e.kind === 'package') externals.set(e.id, (externals.get(e.id) ?? 0) + 1);
  }
});
const linked = new Set(imports.flatMap((e) => [e.source, e.target]));
const group = (id) => (id.includes('/') ? id.split('/')[0] : '.');
const base = (id) => basename(id);
const dup = new Map();
for (const n of ranked) dup.set(base(n.id), (dup.get(base(n.id)) ?? 0) + 1);
const files = ranked
  .filter((n) => linked.has(n.id))
  .map((n) => ({
    id: n.id,
    label: dup.get(base(n.id)) > 1 ? (dirname(n.id) === '.' ? n.id : `${basename(dirname(n.id))}/${base(n.id)}`) : base(n.id),
    group: group(n.id),
    fanIn: n.fanIn,
    fanOut: n.fanOut,
  }));
const fileIds = new Set(files.map((n) => n.id));
const isolated = ranked.length - files.length;

// Functions: probe each hub file's functions and methods for callees, keep the busiest callers.
// Every ranked finding is kept (data.json is the record; the brief shows the top of it).
const [cyc, iss] = await Promise.all([q('cycles', '--limit', '100'), q('issues', '--limit', '1000')]);
// Expand the busiest files and the files that ranked findings name, so questions can name their owners.
const flagged = iss.results.filter((r) => ['god-file', 'critical-file', 'single-point-of-failure', 'misplaced-file'].includes(r.detector)).map((r) => files.find((n) => n.id === r.subject)).filter(Boolean).slice(0, 4);
const hubs = [...new Set([...[...files].sort((a, b) => b.fanIn - a.fanIn).slice(0, hubFiles), ...flagged])];
const isTest = (id) => /(^|\/)tests?(\/|\.|#)|#tests/.test(id);
const symbols = new Map();
const callees = new Map();
// Function edges can also come from the language server (resolved call hierarchy). `both` labels each edge graph, lsp or both.
const keyOf = (file, name) => `${file}#${String(name).split('.').pop().replace(/@\d+$/, '')}`;
const lspMap = new Map();
let lspState = f.calls === 'both' ? (frozen ? 'skipped: section was graphed from a frozen copy' : 'pending') : 'off';
async function lspCallees(x, file) {
  if (lspState !== 'ok') return null;
  try {
    const r = await octocode(['lspSearch', '--json', JSON.stringify({ queries: [{ operation: 'callees', path: join(snap.root, file), symbolName: x.name, line: x.line }] })], { timeout: lspTimeout });
    const d = r.results?.[0]?.data;
    if (!d || d.errorCode) return null;
    if (d.kind === 'empty') return d.isPartial ? null : new Map(); // complete analysis with no callees is an answer; partial analysis is not
    const out = new Map();
    for (const fl of d.files ?? []) for (const m of fl.matches ?? []) {
      const def = relative(snap.root, resolve(m.path ?? fl.path));
      out.set(keyOf(def, m.symbolName), { file: def, name: String(m.symbolName).split('.').pop() });
    }
    return out;
  } catch { return null; }
}
if (lspState === 'pending') {
  try {
    await octocode(['lspSearch', '--json', JSON.stringify({ queries: [{ operation: 'documentSymbols', path: join(snap.root, [...files].sort((a, b) => b.fanIn - a.fanIn)[0].id) }] })], { timeout: lspTimeout });
    lspState = 'ok';
  } catch (e) {
    lspState = `skipped: no answer within ${f['lsp-timeout']} s (cold start or no server): rerun when warm`;
  }
}
await pool(hubs, Math.max(1, Math.floor(jobs / 2)), async (h) => {
  const r = await q('walk', h.id, '--edge', 'contains', '--direction', 'out', '--depth', '2', '--limit', '1000');
  const cands = r.results
    .filter((x) => ['function', 'method'].includes(x.symbolKind) && !isTest(x.id))
    .sort((a, b) => Number(!!b.exported) - Number(!!a.exported) || a.line - b.line)
    .slice(0, probe);
  const probed = await pool(cands, Math.max(2, Math.floor(jobs / 2)), async (x) => ({ x, c: (await q('callees', x.id, '--limit', '200')).results.map((t) => t.id), l: await lspCallees(x, h.id) }));
  probed.sort((a, b) => b.c.length - a.c.length || Number(!!b.x.exported) - Number(!!a.x.exported));
  for (const { x, c, l } of probed.slice(0, perFile)) {
    lspMap.set(x.id, l);
    symbols.set(x.id, { id: x.id, file: h.id, name: x.name, kind: x.symbolKind, line: x.line, exported: !!x.exported, calls: c.length });
    callees.set(x.id, c);
  }
});
let calls = [];
const stats = { probed: callees.size, resolved: 0, both: 0, graphOnly: 0, lspOnly: 0 };
let extra = 0;
const addNode = (id, file, name) => {
  if (symbols.has(id)) return true;
  if (extra >= 40) return false;
  extra++;
  symbols.set(id, { id, file, name, kind: 'callee', calls: 0, exported: false });
  return true;
};
for (const [from, targets] of callees) {
  const L = lspMap.get(from);
  const gKeys = new Set(targets.map((t) => keyOf(t.split('#')[0], t.split('#')[1] ?? '')));
  if (L) {
    stats.resolved++;
    for (const k of gKeys) if (L.has(k)) stats.both++; else stats.graphOnly++;
    for (const k of L.keys()) if (!gKeys.has(k)) stats.lspOnly++;
  }
  for (const t of targets) {
    const tf = t.split('#')[0];
    const via = L ? (L.has(keyOf(tf, t.split('#')[1] ?? '')) ? 'both' : 'graph') : 'graph';
    if (t === from || !fileIds.has(tf)) continue;
    calls.push({ source: from, target: addNode(t, tf, (t.split('#')[1] ?? '').split('.').pop()) ? t : tf, via });
  }
  for (const [k, def] of L ?? []) {
    if (gKeys.has(k) || !fileIds.has(def.file) || isTest(def.file)) continue;
    const id = [...symbols.values()].find((x) => keyOf(x.file, x.id.split('#')[1] ?? '') === k)?.id ?? `${def.file}#${def.name}`;
    if (id !== from && addNode(id, def.file, def.name)) calls.push({ source: from, target: id, via: 'lsp' });
  }
}
calls = [...new Map(calls.map((c) => [`${c.source}>${c.target}`, c])).values()];
const fnFiles = new Set([...symbols.values()].map((s) => s.file));
for (const c of calls) if (fileIds.has(c.target)) fnFiles.add(c.target);

// Insights come straight from graph analysis; findings are hypotheses to verify.
const crossings = new Map();
for (const e of imports) {
  const [a, b] = [group(e.source), group(e.target)];
  if (a !== b) crossings.set(`${a}>${b}`, (crossings.get(`${a}>${b}`) ?? 0) + 1);
}
const groups = [...crossings].map(([k, count]) => ({ from: k.split('>')[0], to: k.split('>')[1], count }));
const insights = {
  hubs: [...files].sort((a, b) => b.fanIn - a.fanIn).slice(0, 6).map(({ id, fanIn, fanOut }) => ({ id, fanIn, fanOut })),
  health: iss.summary.health,
  findings: iss.summary.byDetector,
  issues: iss.results.map((r) => ({ detector: r.detector, subject: r.subject, title: r.title, score: r.score })),
  cycles: { total: cyc.total, runtime: cyc.results.filter((c) => c.runtime).length, largestRuntime: Math.max(0, ...cyc.results.filter((c) => c.runtime).map((c) => c.size)), top: cyc.results.slice(0, 5).map((c) => ({ size: c.size, runtime: c.runtime, nodes: c.nodes.slice(0, 40) })) },
  groups,
  mutualGroups: groups.filter((c) => c.from < c.to && groups.some((d) => d.from === c.to && d.to === c.from)).map((c) => [c.from, c.to]),
};

const { bytes: sourceBytes, files: sourceFiles } = sourceSize(snap.root, f.exclude.split(',').filter(Boolean));

const legends = {
  files: [`Files: ${files.length}`, `Imports: ${imports.length}`],
  functions: [`Files: ${fnFiles.size}`, `Functions: ${symbols.size}`, `Calls: ${calls.length}`],
};
const data = {
  section: resolve(section),
  frozen,
  snapshot: { id: snap.id, root: snap.root, counts: snap.counts, linkage: snap.linkage?.ratio },
  files,
  imports,
  symbols: [...symbols.values()],
  calls,
  fnFiles: [...fnFiles],
  legends,
  insights,
  hubFiles: hubs.map((h) => h.id),
  externals: [...externals].sort((a, b) => b[1] - a[1]).slice(0, 40).map(([id, count]) => ({ id, count })),
  callStats: { mode: f.calls, lsp: lspState, ...stats },
  tokens: { sourceBytes, sourceFiles },
};
mkdirSync(out, { recursive: true });
writeFileSync(join(out, 'data.json'), JSON.stringify(data));
console.log(JSON.stringify({ ms: { total: Date.now() - t0, ingest: ingestMs }, out, frozen, files: files.length, imports: imports.length, functions: symbols.size, calls: calls.length, isolated, cycles: cyc.total }));
