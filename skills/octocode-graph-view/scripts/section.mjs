// One section (a chunk or the whole root) -> data.json: every file, import, call and declaration of its
// graph snapshot. Each graph page is handed to `emit` as it arrives, so a viewer grows while paging runs.
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { basename, dirname, join, resolve } from 'node:path';
import { graphBuild, readGraphs, totalOf } from './graph-pages.mjs';
import { SKIPPED_DIRS, degrees, pool, prepareGraphRun, topGroup } from './lib.mjs';
import { packageKey } from './stitch.mjs';

const RANK = { high: 3, medium: 2, low: 1 };
const fileOf = (id) => id.split('#')[0];
const STREAMED = { find: 'files', witnesses: 'witnesses', symbols: 'symbols' };
const group = topGroup;

// graphBuild refuses a tree that changes while it runs; the frozen copy lives under the result directory.
function freeze(dir, out) {
  mkdirSync(join(out, 'tmp'), { recursive: true });
  const dst = mkdtempSync(join(out, 'tmp', 'frozen-'));
  execFileSync('rsync', ['-a', '--delete', ...SKIPPED_DIRS.flatMap((d) => ['--exclude', d]), `${resolve(dir)}/`, `${dst}/`]);
  return dst;
}

// The snapshot of `section` (an absolute directory inside the workspace) and the path its queries repeat.
async function ingest(section, out, exclude, onProgress) {
  const run = async (path) => ({ path, snap: await graphBuild({ path, ...(exclude.length ? { exclude: exclude } : {}) }, { onProgress }) });
  try {
    return { ...(await run(section)), frozen: false };
  } catch (e) {
    if (!/changed during graph analysis/.test(e.message)) throw e;
    return { ...(await run(freeze(section, out))), frozen: true };
  }
}

// One edge per (from, to): every witness line, occurrence count, strongest confidence, resolutions seen, and
// `test` when every witness is test code (a test file, or an item compiled only under test).
export function foldEdges(witnesses, kind) {
  const by = new Map();
  for (const w of witnesses.filter((x) => x.edge === kind)) {
    const k = `${w.from}>${w.to}`;
    const e = by.get(k) ?? { source: w.from, target: w.to, line: w.line, lines: [], count: 0, confidence: w.confidence, resolution: [], test: true };
    e.count++;
    e.test &&= !!w.test;
    if (Number.isInteger(w.line) && !e.lines.includes(w.line)) e.lines.push(w.line);
    if (w.line < e.line) e.line = w.line;
    if ((RANK[w.confidence] ?? 0) > (RANK[e.confidence] ?? 0)) e.confidence = w.confidence;
    if (w.detail && !e.resolution.includes(w.detail)) e.resolution.push(w.detail);
    by.set(k, e);
  }
  for (const e of by.values()) e.lines.sort((a, b) => a - b);
  return [...by.values()];
}

const label = (id, dup) => (dup.get(basename(id)) > 1 ? (dirname(id) === '.' ? id : `${basename(dirname(id))}/${basename(id)}`) : basename(id));

function fileNodes(found, imports) {
  const { fanIn, fanOut } = degrees(imports);
  const dup = new Map();
  for (const n of found) dup.set(basename(n.id), (dup.get(basename(n.id)) ?? 0) + 1);
  return found.map((n) => ({ id: n.id, label: label(n.id, dup), group: topGroup(n.id), language: n.language, roles: n.roles ?? [], fanIn: fanIn.get(n.id) ?? 0, fanOut: fanOut.get(n.id) ?? 0 })).toSorted((a, b) => a.id.localeCompare(b.id));
}

function externalsOf(importEdges) {
  const externals = new Map();
  for (const e of importEdges.filter((x) => x.target.startsWith('pkg:'))) {
    const x = externals.get(e.target) ?? { id: e.target, count: 0, files: [] };
    x.count++;
    x.files.push(e.source);
    externals.set(e.target, x);
  }
  return [...externals.values()].toSorted((a, b) => b.count - a.count);
}

// Declarations of every file (stitching binds calls between chunks to them); symbols are those in a call.
function symbolRows(symbolNodes, fileIds, calls) {
  const inCalls = new Set(calls.flatMap((c) => [c.source, c.target]).filter((id) => id.includes('#')));
  const declarations = [];
  const symbols = new Map();
  for (const x of symbolNodes) {
    if (!fileIds.has(x.path)) continue;
    const d = { id: x.id, file: x.path, name: x.symbolName, kind: x.kind, line: x.line, endLine: x.endLine, exported: !!x.exported };
    declarations.push(d);
    if (inCalls.has(x.id)) symbols.set(x.id, { ...d, calls: 0 });
  }
  for (const id of inCalls) if (!symbols.has(id)) symbols.set(id, { id, file: fileOf(id), name: id.split('#')[1].split('.').pop().replace(/@\d+$/, ''), kind: 'symbol', calls: 0, exported: false });
  for (const c of calls) if (symbols.has(c.source)) symbols.get(c.source).calls++;
  declarations.sort((a, b) => a.file.localeCompare(b.file) || (a.line ?? 0) - (b.line ?? 0));
  return { declarations, symbols: [...symbols.values()] };
}

async function batchedWalk(queries, max, jobs, walk) {
  const batches = [];
  for (let i = 0; i < queries.length; i += max) batches.push(queries.slice(i, i + max));
  return (await pool(batches, jobs, walk)).flat();
}

// Call gaps of the files that import another chunk (past this root, or another chunk's workspace package): the
// only sites stitching can bind (stitch.mjs bindCalls binds through those imports). The engine keeps only the
// `not-declared-in-repo` call sites of those files, before paging, in concurrent file groups.
// With no other chunk (`crossPackages` null) there is nothing to bind.
async function crossCallGaps(walk, { outsideRoot, externals, crossPackages, jobs, maxTargets, graphBatch }) {
  if (!crossPackages) return [];
  const files = [...new Set([...outsideRoot.map((o) => o.file), ...externals.filter((x) => crossPackages.has(packageKey(x.id))).flatMap((x) => x.files)])];
  if (!files.length) return [];
  const max = maxTargets ?? (await prepareGraphRun()).maxTargets;
  const groups = [];
  for (let i = 0; i < files.length; i += max) groups.push(files.slice(i, i + max));
  const queries = groups.map((target) => ({ operation: 'unresolved', target, edges: ['calls'], reasons: ['not-declared-in-repo', 'external-package'], limit: 1000 }));
  const pages = await batchedWalk(queries, graphBatch, jobs, walk);
  return pages.flatMap((p) => p.rows)
    .filter((u) => u.targetName)
    .map((u) => ({ from: u.from, file: u.path, targetName: u.targetName, line: u.line, reason: u.reason }));
}

function insightsOf(files, imports, cyc, iss) {
  const crossings = new Map();
  for (const e of imports) {
    const k = `${group(e.source)}>${group(e.target)}`;
    if (group(e.source) !== group(e.target)) crossings.set(k, (crossings.get(k) ?? 0) + 1);
  }
  const groups = [...crossings].map(([k, count]) => ({ from: k.split('>')[0], to: k.split('>')[1], count }));
  const runtime = cyc.rows.filter((c) => c.runtime);
  return {
    hubs: files.toSorted((a, b) => b.fanIn - a.fanIn).slice(0, 6).map(({ id, fanIn, fanOut }) => ({ id, fanIn, fanOut })),
    health: iss.summary?.health ?? {},
    findings: iss.summary?.byDetector ?? {},
    issues: iss.rows,
    cycles: { all: cyc.rows, total: cyc.rows.length, runtime: runtime.length, largestRuntime: Math.max(0, ...runtime.map((c) => c.size)), top: cyc.rows.slice(0, 5).map((c) => ({ size: c.size, runtime: c.runtime, nodes: c.nodes })) },
    groups,
    mutualGroups: groups.filter((c) => c.from < c.to && groups.some((d) => d.from === c.to && d.to === c.from)).map((c) => [c.from, c.to]),
  };
}

// A target is a source file in both evidence operations. Each group owns its native continuation chain.
export async function readSectionRows({ counts, jobs, graphBatch, fileGroup, walk }) {
  const fields = [
    { operation: 'find', kind: 'file' }, { operation: 'diagnostics' }, { operation: 'cycles' },
    { operation: 'issues' }, { operation: 'witnesses', edges: ['imports', 'calls'], confidence: 'low' },
    { operation: 'symbols' },
  ].map((row) => ({ ...row, limit: Math.max(1, counts.nodes, counts.edges) }));
  const count = counts.nodesByKind?.file;
  if (!(count > fileGroup && jobs > 1)) return { pages: await batchedWalk(fields, graphBatch, jobs, walk), groups: 1 };
  const overview = await batchedWalk(fields.slice(0, 4), graphBatch, jobs, walk);
  const files = [...new Set(overview[0].rows.map(f => f.id))];
  const evidence = fields.slice(4);
  if (files.length !== count) return { pages: [...overview, ...await batchedWalk(evidence, graphBatch, jobs, walk)], groups: 1 };
  const batches = [];
  for (let i = 0; i < files.length; i += fileGroup) {
    const target = files.slice(i, i + fileGroup);
    const queries = evidence.map(row => ({ ...row, target }));
    for (let j = 0; j < queries.length; j += graphBatch) batches.push(queries.slice(j, j + graphBatch));
  }
  const results = await pool(batches, jobs, walk);
  const combined = new Map(evidence.map(row => [row.operation, { rows: [] }]));
  for (const [index, pages] of results.entries()) for (const [k, page] of pages.entries()) {
    const output = combined.get(batches[index][k].operation);
    for (const row of page.rows) output.rows.push(row);
  }
  return { pages: [...overview, ...combined.values()], groups: Math.ceil(files.length / fileGroup) };
}

// emit(kind, rows, page): kind is `files`, `witnesses` or `symbols` (rows keep the graph's own fields), `build` (one
// graphBuild progress line in page.line) or `snapshot`.
// crossPackages: package keys of the other chunks (null when this section is the only chunk).
export async function buildSection(section, { out, jobs = 8, exclude = [], emit = () => {}, crossPackages = null, graphBatch = 1, maxTargets, fileGroup }) {
  const evidence = join(out, 'tmp', 'evidence');
  mkdirSync(evidence, { recursive: true });
  const t0 = Date.now();
  const { path, snap, frozen } = await ingest(section, out, exclude, (line) => emit('build', [], { line }));
  const ingestMs = Date.now() - t0;
  // Evidence is kept whole and gzipped (lossless; read with `gzip -dc`): raw pages run to hundreds of MB.
  writeFileSync(join(evidence, 'ingest.json.gz'), gzipSync(JSON.stringify(snap)));
  // Every query repeats the build's path and pins its snapshot id.
  const walk = (fields) => {
    const queries = fields.map((row) => ({ ...row, path, graph: snap.graph }));
    const file = join(evidence, `${createHash('sha256').update(JSON.stringify(queries)).digest('hex')}.jsonl.gz`);
    let pages = 0;
    const save = (input, response) => {
      pages++;
      appendFileSync(file, gzipSync(`${JSON.stringify({ input, response })}\n`));
    };
    return readGraphs(queries, { save, snapshotRoot: snap.root, onPage: (page, index) => {
      const stream = STREAMED[queries[index].operation];
      if (stream && page.rows?.length) emit(stream, page.rows, { page: pages, total: totalOf(page) });
    } });
  };
  emit('snapshot', [], { id: snap.graph, ingestMs, frozen, counts: snap.counts, linkage: snap.linkage?.ratio });
  const { pages, groups } = await readSectionRows({ counts: snap.counts, jobs, graphBatch, fileGroup, walk });
  const [found, outside, cyc, iss, rel, sym] = pages;
  const fileIds = new Set(found.rows.map((n) => n.id));
  const importEdges = foldEdges(rel.rows, 'imports');
  const imports = importEdges.filter((e) => fileIds.has(e.source) && fileIds.has(e.target));
  const files = fileNodes(found.rows, imports);
  const externals = externalsOf(importEdges);
  const outsideRoot = outside.rows.filter((d) => d.code === 'unresolved-internal' || d.code === 'outside-root').map((d) => ({ file: d.path, line: d.line, specifier: d.message.match(/"(.*)"/)?.[1] ?? null, message: d.message }));
  const callEdges = foldEdges(rel.rows, 'calls');
  const calls = callEdges.filter((e) => fileIds.has(fileOf(e.source)) && fileIds.has(fileOf(e.target))).map((e) => ({ ...e, via: 'graph' }));
  const unresolvedCalls = await crossCallGaps(walk, { outsideRoot, externals, crossPackages, jobs, graphBatch, maxTargets });
  const { declarations, symbols } = symbolRows(sym.rows, fileIds, calls);
  const linked = new Set(imports.flatMap((e) => [e.source, e.target]));
  const snapshot = { ...found.graph, id: snap.graph, root: resolve(path), queryRoot: snap.root, counts: snap.counts, linkage: snap.linkage?.ratio };
  const partialReasons = snapshot.partialReasons ?? [];
  snapshot.isPartial = snapshot.isPartial === true || partialReasons.length > 0;
  const data = {
    section: resolve(section),
    evidence: 'tmp/evidence',
    groupWalks: Object.fromEntries([['witnesses', rel], ['symbols', sym]].map(([op, w]) => [op, { groups, grouped: groups > 1, rows: w.rows.length, total: w.rows.length }])),
    coverage: { isPartial: snapshot.isPartial, partialReasons, files: files.length, isolated: files.filter((n) => !linked.has(n.id)).length, imports: imports.length, calls: calls.length, callsOutside: callEdges.length - calls.length, outsideRoot: outsideRoot.length, declarations: declarations.length, symbolsInCalls: symbols.length },
    frozen,
    ms: { ingest: ingestMs, total: Date.now() - t0 },
    snapshot,
    files,
    imports,
    symbols,
    calls,
    fnFiles: [...new Set([...symbols.map((s) => s.file), ...calls.flatMap((c) => [fileOf(c.source), fileOf(c.target)])])],
    insights: insightsOf(files, imports, cyc, iss),
    externals,
    outsideRoot,
    declarations,
    unresolvedCalls,
  };
  writeFileSync(join(out, 'data.json'), JSON.stringify(data));
  return data;
}
