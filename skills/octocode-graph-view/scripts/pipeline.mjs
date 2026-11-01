// The whole run as one event stream: list files (structureSearch pages), plan chunks, ingest them in parallel
// (graph pages stream as rows), stitch, cross-check calls with LSP, check files, compute graph metrics, write data.json.
// The live server and the headless build consume the same events; data.json is their fold.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { availableParallelism } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { onToolCall } from './lib.mjs';
import { planChunks, refine, name } from './plan.mjs';
import { buildSection } from './section.mjs';
import { chunkOf, packageKey, stitch } from './stitch.mjs';
import { computeMetrics } from './metrics.mjs';
import { describeCall, pagedTool, responseShape, rowsOf } from './tools.mjs';
import { writeBriefs } from './render.mjs';
import { lspPhase } from './lsp.mjs';
import { verifyPhase } from './verify.mjs';
import { buildWiki } from './wiki.mjs';

// `jobs`: concurrent tool calls per chunk. The warm server runs graph pages in parallel: on 12 cores one page took
// 0.065 s alone and 0.015 s of wall time each at 12 concurrent calls, so throughput follows the cores.
export const defaults = () => ({ jobs: availableParallelism(), ingestJobs: Math.max(1, Math.floor(availableParallelism() / 4)), lsp: 'on', whole: false, exclude: [] });

const prefixOf = (path) => (path === '.' ? '' : `${path}/`);
const fileOf = (id) => id.split('#')[0];

// Tool calls become `tool` events: what ran, which page or cursor, rows, time, bytes, errors.
// Tool ids are unique across every run and refresh that share one events.jsonl: a per-hook run prefix, because the
// call counter restarts in each process.
export function reportTools(emit) {
  const run = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  onToolCall(({ id: n, state, args, ms, bytes, error, value }) => {
    const call = describeCall(args);
    const id = `${run}-${n}`;
    if (state === 'start') emit({ type: 'tool', id, state, ...call });
    else emit({ type: 'tool', id, state: error ? 'error' : 'done', ...call, ms, bytes, ...responseShape(value), ...(error ? { error } : {}) });
  });
}

// File skeleton from structureSearch, page by page; paths come back workspace-relative.
export async function listFiles(root, emit) {
  const files = [];
  try {
    await pagedTool('structureSearch', { queries: [{ operation: 'files', path: root, entryType: 'file', view: 'paths' }] }, {
      onPage: (page, n) => {
        const rows = rowsOf([page], 'files').flatMap((g) => (g.files ?? []).map((f) => relative(root, resolve(g.dir, f))));
        files.push(...rows);
        emit({ type: 'skeleton', files: rows, page: n, total: page.results?.[0]?.data?.pagination?.totalItems });
      },
    });
  } catch (error) {
    // git lists the same tracked and unignored files when structureSearch is unavailable.
    emit({ type: 'note', level: 'warn', text: `structureSearch listing failed, using git ls-files: ${error.message}` });
    const rows = execFileSync('git', ['-C', root, 'ls-files', '--cached', '--others', '--exclude-standard'], { encoding: 'utf8', maxBuffer: Infinity }).split('\n').filter((p) => p && existsSync(join(root, p)));
    files.push(...rows);
    emit({ type: 'skeleton', files: rows, page: 1, total: rows.length });
  }
  return [...new Set(files)].filter((p) => !p.startsWith('..'));
}

export function manifestName(root, dir) {
  try {
    const pj = join(root, dir, 'package.json');
    if (existsSync(pj)) return JSON.parse(readFileSync(pj, 'utf8')).name;
    const ct = join(root, dir, 'Cargo.toml');
    if (existsSync(ct)) return readFileSync(ct, 'utf8').match(/^name\s*=\s*"([^"]+)"/m)?.[1];
  } catch { /* a chunk without a readable manifest has no package name */ }
  return undefined;
}

// Graph rows with ids relative to the repository root, so every chunk's pages share one id space.
function rebase(kind, rows, prefix) {
  const p = (id) => (typeof id === 'string' && !id.startsWith('pkg:') ? prefix + id : id);
  if (kind === 'files') return rows.map((n) => ({ id: p(n.id), language: n.language, roles: n.roles ?? [] }));
  if (kind === 'symbols') return rows.map((x) => ({ id: p(x.id), file: p(x.file), name: x.name, kind: x.symbolKind, line: x.line, endLine: x.endLine, exported: !!x.exported }));
  return rows.map((w) => ({ edge: w.edge, from: p(w.from), to: p(w.to), file: p(w.file), line: w.line, confidence: w.confidence, detail: w.detail, test: !!w.test }));
}

// One chunk's snapshot and graph pages; `ctx.datas` receives its data. A chunk the graph refuses as too broad
// is refined into parts that replace it in `ctx.chunks` and are returned for building.
export async function buildChunk(c, ctx) {
  const { root, out, opts, emit } = ctx;
  const t0 = Date.now();
  emit({ type: 'chunk', id: c.id, state: 'ingesting' });
  const onRows = (kind, rows, page) => {
    if (kind === 'snapshot') emit({ type: 'chunk', id: c.id, state: 'paging', snapshot: page });
    else if (kind === 'build') emit({ type: 'chunk', id: c.id, state: 'ingesting', build: page.line });
    else emit({ type: 'rows', chunk: c.id, kind, rows: rebase(kind, rows, prefixOf(c.path)), page: page.page, total: page.total });
  };
  try {
    const others = ctx.chunks.filter((x) => x !== c);
    const crossPackages = others.length ? new Set(others.filter((x) => x.package).map((x) => packageKey(x.package))) : null;
    const d = await buildSection(join(root, c.path), { out: join(out, 'chunks', c.id), jobs: opts.jobs, exclude: [...opts.exclude, ...(c.exclude ?? [])], emit: onRows, crossPackages });
    ctx.datas.set(c.id, d);
    emit({ type: 'chunk', id: c.id, state: 'done', ms: Date.now() - t0, counts: d.coverage });
    return [];
  } catch (error) {
    const parts = /scopeTooBroad/.test(error.message) ? refine(c, ctx.files) : null;
    if (!parts) {
      c.error = error.message.split('\n')[0];
      emit({ type: 'chunk', id: c.id, state: 'error', ms: Date.now() - t0, error: c.error });
      return [];
    }
    name(parts, ctx.used);
    for (const p of parts) p.package = manifestName(root, p.path);
    ctx.chunks.splice(ctx.chunks.indexOf(c), 1, ...parts);
    emit({ type: 'chunk', id: c.id, state: 'split', ms: Date.now() - t0, into: parts.map(({ id, path, files, loose }) => ({ id, path, files, loose })) });
    return parts;
  }
}

// A dynamic queue, largest first. Chunk builds overlap (the engine's workspace lock covers only publication,
// ast_graph/store/build_lock.rs), so the run takes as long as its longest slot: one slot always holds the largest
// remaining chunk, which bounds the total, and the other slots take the smallest so the viewer fills in early.
// Refined parts join the queue while the other chunks keep building.
const bySize = (a, b) => b.files - a.files;
async function buildAll(ctx) {
  const queue = [...ctx.chunks].toSorted(bySize);
  const running = new Set();
  let done = 0;
  await new Promise((finish) => {
    const next = () => {
      if (ctx.signal?.aborted) queue.length = 0;
      if (!queue.length && !running.size) return finish();
      while (running.size < ctx.opts.ingestJobs && queue.length) {
        const holdsLargest = [...running].some((r) => r.files >= queue[0].files);
        const c = holdsLargest ? queue.pop() : queue.shift();
        running.add(c);
        void buildChunk(c, ctx).then((parts) => {
          queue.push(...parts);
          queue.sort(bySize);
          running.delete(c);
          done += parts.length ? 0 : 1;
          ctx.emit({ type: 'progress', phase: 'graph', done, total: ctx.chunks.length });
          next();
        });
      }
    };
    next();
  });
}

// Every node and edge carries where it lives: repository-relative path, absolute path and lines.
export function locate(D, root) {
  const at = (path, extra) => ({ path, abs: join(root, path), ...extra });
  for (const f of D.files) f.loc = f.kind === 'package' ? at(f.manifest ?? f.id, { line: 1 }) : at(f.id, { line: 1 });
  for (const s of D.symbols) s.loc = at(s.file, { line: s.line ?? 1, endLine: s.endLine ?? s.line ?? 1 });
  for (const e of D.imports) e.loc = at(fileOf(e.source), { lines: e.lines ?? (e.line ? [e.line] : []) });
  for (const e of D.calls) e.loc = at(fileOf(e.source), { lines: e.lines ?? (e.line ? [e.line] : []) });
  return D;
}

function packageManifests(D, chunks, root) {
  for (const f of D.files.filter((x) => x.kind === 'package')) {
    const owner = chunks.find((c) => c.id === f.chunk);
    const dir = owner?.path ?? '.';
    f.manifest = ['package.json', 'Cargo.toml'].map((m) => (dir === '.' ? m : `${dir}/${m}`)).find((m) => existsSync(join(root, m)));
  }
}

export function stitchPhase(ctx) {
  const { root, chunks, datas, files, emit } = ctx;
  const s = stitch({ root, chunks, datas, files });
  const D = s.whole;
  // One chunk: keep its section-wide health, which stitching cannot merge across chunks.
  if (datas.size === 1) D.insights.health = [...datas.values()][0].insights.health;
  D.uncoveredFiles = files.filter((p) => !chunkOf(p, chunks));
  D.chunks = chunks.map(({ id, path, files: n, loose, package: pkg, error }) => ({ id, path, files: n, loose, package: pkg, built: datas.has(id), error }));
  packageManifests(D, chunks, root);
  emit({ type: 'stitched', imports: D.imports.filter((e) => e.crossChunk), calls: D.calls.filter((e) => e.crossChunk), packages: D.files.filter((f) => f.kind === 'package').map(({ id, label, chunk, manifest }) => ({ id, label, chunk, manifest })), unresolved: s.unresolved, uncovered: D.uncoveredFiles });
  return D;
}

// After stitching: LSP cross-check (when `lsp.run`; `lsp.note` says why not), the file check, graph metrics,
// data.json and the briefs. Shared by a full run and a refresh, so both emit the same events.
export async function finishRun(D, ctx, phase, lsp) {
  const { root, out, opts, emit, signal } = ctx;
  if (!lsp.run || signal?.aborted) {
    D.callStats = { mode: 'graph', lsp: 'off' };
    phase('lsp', 'skipped', { note: signal?.aborted ? 'cancelled' : lsp.note });
  } else {
    phase('lsp', 'start', lsp.start ? { note: lsp.start } : {});
    D.callStats = await lspPhase(D, { root, emit, signal, jobs: opts.jobs });
    phase('lsp', D.callStats.lsp === 'ok' ? 'done' : 'skipped', { note: D.callStats.lsp });
  }
  phase('verify', 'start', { total: D.files.length });
  const checked = await verifyPhase(D, { root, chunks: ctx.chunks, datas: ctx.datas, emit, jobs: opts.jobs });
  phase('verify', 'done', { note: `${checked.ok}/${checked.checked} match their files${checked.stale ? `, ${checked.stale} stale` : ''}${checked.missing ? `, ${checked.missing} missing` : ''}${checked.lines ? `, ${checked.lines} with lines outside the file` : ''}` });
  phase('metrics', 'start');
  locate(D, root);
  computeMetrics(D);
  emit(metricsEvent(D));
  writeFileSync(join(out, 'data.json'), JSON.stringify(D));
  const briefs = writeBriefs(D, out);
  phase('metrics', 'done', { note: `${D.metrics.stats.nodes} nodes, ${D.metrics.stats.edges} weighted edges in ${Object.values(D.metrics.timings).reduce((a, x) => a + x, 0)} ms` });
  wikiPhase(D, ctx);
  return briefs;
}

// The code wiki (wiki.mjs) follows the new communities: pages keep their slugs and notes, a refresh passes the files
// it found changed (`ctx.changed`) so their pages' notes go stale. A wiki failure never fails the run.
function wikiPhase(D, { out, emit, changed }) {
  try {
    const { pages, updated, created, removed } = buildWiki({ D, out, changed });
    emit({ type: 'wiki', pages, updated, created, removed });
  } catch (error) {
    emit({ type: 'note', level: 'warn', text: `wiki not updated: ${String(error.message).split('\n')[0]}` });
  }
}

export async function runPipeline({ root, out, opts, emit, signal }) {
  const t0 = Date.now();
  const phase = (id, state, extra = {}) => emit({ type: 'phase', id, state, ms: Date.now() - t0, ...extra });
  reportTools(emit);
  mkdirSync(out, { recursive: true });
  emit({ type: 'run', root, name: root.split('/').pop(), out, options: opts, startedAt: new Date().toISOString() });
  phase('list', 'start');
  // --exclude prunes directory names everywhere, as ingest does, so excluded trees are never planned.
  const excluded = new Set(opts.exclude);
  const files = (await listFiles(root, emit)).filter((p) => !p.split('/').slice(0, -1).some((d) => excluded.has(d)));
  phase('list', 'done', { files: files.length });
  phase('plan', 'start');
  const chunks = planChunks(files, { whole: opts.whole });
  for (const c of chunks) c.package = manifestName(root, c.path);
  emit({ type: 'plan', chunks: chunks.map(({ id, path, files: n, loose, package: pkg }) => ({ id, path, files: n, loose, package: pkg })), whole: opts.whole });
  phase('plan', 'done', { chunks: chunks.length });
  const ctx = { root, out, opts, emit, signal, files, chunks, datas: new Map(), used: new Set(chunks.map((c) => c.id)) };
  phase('graph', 'start', { total: chunks.length });
  await buildAll(ctx);
  phase('graph', 'done', { built: ctx.datas.size, total: chunks.length });
  if (!ctx.datas.size) throw new Error(`No chunk was built: ${chunks.map((c) => `${c.id}: ${c.error}`).join('; ')}`);
  phase('stitch', 'start');
  const D = stitchPhase(ctx);
  phase('stitch', 'done', { crossImports: D.coverage.crossChunkImports, crossCalls: D.coverage.crossChunkCalls });
  const briefs = await finishRun(D, ctx, phase, { run: opts.lsp !== 'off', note: '--lsp off' });
  emit({ type: 'done', ms: Date.now() - t0, data: join(out, 'data.json'), brief: briefs.brief, coverage: D.coverage });
  return D;
}

// Per-file metrics as columns aligned to one id list, weighted edges as index columns: compact for the stream.
const COLUMNS = ['pagerank', 'betweenness', 'inWeight', 'outWeight', 'depth', 'core', 'community', 'cycle'];
export function metricsEvent(D) {
  const ids = D.files.map((f) => f.id);
  const at = new Map(ids.map((id, i) => [id, i]));
  const edges = D.fileEdges.filter((e) => at.has(e.source) && at.has(e.target));
  const { cycles, communities, packages, stats, timings, weightRule } = D.metrics;
  return {
    type: 'metrics', ids, weightRule, stats, timings, cycles, packages,
    columns: Object.fromEntries(COLUMNS.map((k) => [k, D.files.map((f) => f.metrics[k])])),
    edges: { source: edges.map((e) => at.get(e.source)), target: edges.map((e) => at.get(e.target)), weight: edges.map((e) => e.weight), test: edges.map((e) => (e.test ? 1 : 0)) },
    communities: communities.filter((c) => c.size > 1),
    coverage: D.coverage, callStats: D.callStats, verification: D.verification,
  };
}

// On-demand AST outline of one file (astSearch symbols, every page).
export async function astOutline(root, file) {
  const pages = await pagedTool('astSearch', { queries: [{ operation: 'symbols', path: join(root, file) }] });
  const error = pages.flatMap((p) => p.results ?? []).find((r) => r?.data?.errorCode)?.data;
  return { symbols: rowsOf(pages, 'symbols'), error: error ? `${error.errorCode}: ${error.error ?? ''}` : undefined };
}

// On-demand LSP question about one symbol: callers, callees, references or definition, every page.
export async function lspQuery(root, { file, symbolName, line, operation }) {
  const pages = await pagedTool('lspSearch', { queries: [{ operation, path: join(root, file), symbolName, line }] });
  const rows = rowsOf(pages, 'files').flatMap((g) => (g.matches ?? []).map((m) => ({ path: relative(root, resolve(m.path ?? g.path)), symbolName: m.symbolName, kind: m.kind, line: m.line, endLine: m.endLine, sites: m.sites })));
  const error = pages.flatMap((p) => p.results ?? []).find((r) => r?.data?.errorCode)?.data;
  return { rows, error: error ? `${error.errorCode}: ${error.error ?? ''}` : undefined };
}

// Saved events of a finished run, for replay without recomputing.
export function readEvents(dir) {
  const file = join(dir, 'events.jsonl');
  if (!existsSync(file)) throw new Error(`no events.jsonl in ${dir} (${readdirSync(dir).join(', ')})`);
  return readFileSync(file, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
}

