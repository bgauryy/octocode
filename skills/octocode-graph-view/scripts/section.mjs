// One section (a chunk or the whole root) -> data.json: every file, import, call and declaration of its
// graph snapshot. Each graph page is handed to `emit` as it arrives, so a viewer grows while paging runs.
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { basename, dirname, join, resolve } from 'node:path';
import { graphBuild, graphTotal, readGraph, sourcesPerQuery, totalOf } from './graph-pages.mjs';
import { SKIPPED_DIRS, degrees, pool, topGroup } from './lib.mjs';
import { packageKey } from './stitch.mjs';

const RANK = { high: 3, medium: 2, low: 1 };
const fileOf = (id) => id.split('#')[0];
const STREAMED = { find: true, witnesses: true, symbols: true };
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
  const run = async (path) => ({ path, snap: await graphBuild({ path, ...(exclude.length ? { exclude } : {}) }, { onProgress }) });
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
    if (!fileIds.has(x.file)) continue;
    const d = { id: x.id, file: x.file, name: x.name, kind: x.symbolKind, line: x.line, endLine: x.endLine, exported: !!x.exported };
    declarations.push(d);
    if (inCalls.has(x.id)) symbols.set(x.id, { ...d, calls: 0 });
  }
  for (const id of inCalls) if (!symbols.has(id)) symbols.set(id, { id, file: fileOf(id), name: id.split('#')[1].split('.').pop().replace(/@\d+$/, ''), kind: 'symbol', calls: 0, exported: false });
  for (const c of calls) if (symbols.has(c.source)) symbols.get(c.source).calls++;
  declarations.sort((a, b) => a.file.localeCompare(b.file) || (a.line ?? 0) - (b.line ?? 0));
  return { declarations, symbols: [...symbols.values()] };
}

// Call gaps of the files that import another chunk (past this root, or another chunk's workspace package): the
// only sites stitching can bind (stitch.mjs bindCalls binds through those imports). The engine keeps only the
// `not-declared-in-repo` call sites of those files, before paging, in concurrent file groups ([`fileGroups`]).
// With no other chunk (`crossPackages` null) there is nothing to bind.
async function crossCallGaps(q, { outsideRoot, externals, crossPackages, jobs }) {
  if (!crossPackages) return [];
  const files = [...new Set([...outsideRoot.map((o) => o.file), ...externals.filter((x) => crossPackages.has(packageKey(x.id))).flatMap((x) => x.files)])];
  if (!files.length) return [];
  const groups = fileGroups(files, jobs, sourcesPerQuery('unresolved'));
  const pages = await pool(groups, jobs, (target) => q(null, { operation: 'unresolved', target, edges: ['calls'], reasons: ['not-declared-in-repo', 'external-package'], limit: 1000 }));
  return pages.flatMap((p) => p.rows)
    .filter((u) => u.targetName)
    .map((u) => ({ from: u.from, file: u.file, targetName: u.targetName, line: u.line, reason: u.reason }));
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

// Sorted source files cut into groups for concurrent multi-target walks: a page is bounded by the response budget,
// so one cursor chain over a large scope is hundreds of calls in a row. About four groups per worker, each at most
// `max` (the schema's target list), so a heavy group does not hold the tail.
export function fileGroups(files, jobs, max) {
  const size = Math.max(1, Math.min(max, Math.ceil(files.length / (jobs * 4))));
  const sorted = files.toSorted();
  const groups = [];
  for (let i = 0; i < sorted.length; i += size) groups.push(sorted.slice(i, i + size));
  return groups;
}

// Every row of a whole-snapshot `witnesses` or `symbols` walk, read as concurrent [`fileGroups`] with the files as
// `target`: a witness belongs to the file holding its source, a symbol to the file containing it, so the groups are
// disjoint. The union is checked against the whole walk's total and each id once; on any mismatch the single walk
// runs instead. `stream` names the rows event the pages feed.
export async function groupWalk(q, stream, files, { jobs, fields, total = graphTotal, max = sourcesPerQuery(fields.operation) }) {
  const whole = await total(fields);
  const groups = fileGroups(files, jobs, max);
  const pages = await pool(groups, jobs, (target) => q(stream, { ...fields, target }));
  const rows = pages.flatMap((p) => p.rows);
  const ids = new Set(rows.map((w) => w.id ?? JSON.stringify(w)));
  if (rows.length === whole && ids.size === whole) return { rows, total: whole, groups: groups.length, grouped: true };
  const single = await q(stream, fields);
  return { rows: single.rows, total: single.rows.length, groups: 1, grouped: false, groupedRows: rows.length };
}

// emit(kind, rows, page): kind is `files`, `witnesses` or `symbols` (rows keep the graph's own fields), `build` (one
// graphBuild progress line in page.line) or `snapshot`.
// crossPackages: package keys of the other chunks (null when this section is the only chunk).
export async function buildSection(section, { out, jobs = 8, exclude = [], emit = () => {}, crossPackages = null }) {
  const evidence = join(out, 'tmp', 'evidence');
  mkdirSync(evidence, { recursive: true });
  const t0 = Date.now();
  const { path, snap, frozen } = await ingest(section, out, exclude, (line) => emit('build', [], { line }));
  const ingestMs = Date.now() - t0;
  // Evidence is kept whole and gzipped (lossless; read with `gzip -dc`): raw pages run to hundreds of MB.
  writeFileSync(join(evidence, 'ingest.json.gz'), gzipSync(JSON.stringify(snap)));
  // Every query repeats the build's path and pins its snapshot id.
  const q = (stream, fields) => {
    const row = { ...fields, path, graph: snap.id };
    const file = join(evidence, `${createHash('sha256').update(JSON.stringify(row)).digest('hex')}.jsonl.gz`);
    const save = (input, response) => {
      // One gzip member per page: concatenated members are one valid gzip stream of JSON lines.
      appendFileSync(file, gzipSync(JSON.stringify({ input, response }) + '\n'));
      const page = response?.results?.[0]?.data ?? {};
      if (stream && STREAMED[row.operation] && page.rows?.length) emit(stream, page.rows, { page: page.pagination?.page ?? 1, total: totalOf(page) });
    };
    return readGraph(row, { save });
  };
  emit('snapshot', [], { id: snap.id, ingestMs, frozen, counts: snap.counts, linkage: snap.linkage?.ratio });
  // Walks that need nothing else start at once, so their cursor chains overlap the witness groups.
  const outsideWalk = q(null, { operation: 'diagnostics', limit: 1000 });
  const independent = Promise.all([
    q(null, { operation: 'cycles', limit: 1000 }),
    q(null, { operation: 'issues', limit: 1000 }),
  ]);
  // Awaited below; a failure surfaces there, not as an unhandled rejection while the witnesses run.
  for (const walk of [outsideWalk, independent]) walk.catch(() => {});
  const found = await q('files', { operation: 'find', kind: 'file', limit: 1000 });
  // Every confidence: each edge keeps its strongest, so a reader filters instead of losing medium and low links.
  const fileList = found.rows.map((n) => n.id);
  const witnessFields = { operation: 'witnesses', path, graph: snap.id, edges: ['imports', 'calls'], confidence: 'low', limit: 1000 };
  const symbolWalk = groupWalk(q, 'symbols', fileList, { jobs, fields: { operation: 'symbols', path, graph: snap.id, limit: 1000 } });
  symbolWalk.catch(() => {});
  const rel = await groupWalk(q, 'witnesses', fileList, { jobs, fields: witnessFields });
  const outside = await outsideWalk;
  const fileIds = new Set(found.rows.map((n) => n.id));
  const importEdges = foldEdges(rel.rows, 'imports');
  const imports = importEdges.filter((e) => fileIds.has(e.source) && fileIds.has(e.target));
  const files = fileNodes(found.rows, imports);
  const externals = externalsOf(importEdges);
  const outsideRoot = outside.rows.filter((d) => d.code === 'unresolved-internal').map((d) => ({ file: d.file, line: d.line, specifier: d.message.match(/"(.*)"/)?.[1] ?? null, message: d.message }));
  const callEdges = foldEdges(rel.rows, 'calls');
  const calls = callEdges.filter((e) => fileIds.has(fileOf(e.source)) && fileIds.has(fileOf(e.target))).map((e) => ({ ...e, via: 'graph' }));
  const [[cyc, iss], sym, unresolvedCalls] = await Promise.all([independent, symbolWalk, crossCallGaps(q, { outsideRoot, externals, crossPackages, jobs })]);
  const { declarations, symbols } = symbolRows(sym.rows, fileIds, calls);
  const linked = new Set(imports.flatMap((e) => [e.source, e.target]));
  const data = {
    section: resolve(section),
    evidence: 'tmp/evidence',
    groupWalks: Object.fromEntries([['witnesses', rel], ['symbols', sym]].map(([op, w]) => [op, { groups: w.groups, grouped: w.grouped, rows: w.rows.length, total: w.total }])),
    coverage: { files: files.length, isolated: files.filter((n) => !linked.has(n.id)).length, imports: imports.length, calls: calls.length, callsOutside: callEdges.length - calls.length, outsideRoot: outsideRoot.length, declarations: declarations.length, symbolsInCalls: symbols.length },
    frozen,
    ms: { ingest: ingestMs, total: Date.now() - t0 },
    snapshot: { id: snap.id, root: resolve(snap.root), counts: snap.counts, linkage: snap.linkage?.ratio },
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
