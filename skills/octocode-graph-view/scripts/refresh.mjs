// Incremental refresh of an existing result directory after code edits: each built chunk's snapshot answers
// graphQuery `stale`, a fresh structureSearch listing names added and removed files, and only the chunks holding a
// change are rebuilt (graphBuild reuses unchanged snapshots). Every chunk is then stitched again and the LSP pass
// (only with `lsp: 'on'`), the file check and graph metrics run over the whole graph, with the same events a full
// run emits, so an open viewer changes in place. A package manifest added or removed changes the chunk plan: the
// whole root runs again. Stateless: everything comes from <out> and the repository.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { readGraph } from './graph-pages.mjs';
import { pool } from './lib.mjs';
import { buildChunk, defaults, finishRun, listFiles, manifestName, readEvents, reportTools, runPipeline, stitchPhase } from './pipeline.mjs';
import { chunkOf } from './stitch.mjs';

const MANIFESTS = new Set(['package.json', 'Cargo.toml']);
const LISTING = 'listing.json';
const prefixOf = (path) => (path === '.' ? '' : `${path}/`);
const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'));
const excludedBy = (exclude) => (p) => !p.split('/').slice(0, -1).some((d) => exclude.has(d));
const ids = (chunks) => chunks.map((c) => c.id);

// A loose chunk excludes, by name, each child directory that holds another chunk (plan.mjs planDir and refine),
// which data.json's chunk rows do not record.
function withExcludes(chunks) {
  return chunks.map((c) => {
    if (!c.loose) return { ...c };
    const prefix = prefixOf(c.path);
    const names = new Set(chunks.filter((o) => o !== c && o.path !== '.' && o.path.startsWith(prefix)).map((o) => o.path.slice(prefix.length).split('/')[0]));
    return { ...c, exclude: [...names] };
  });
}

// The listing data.json was built from: the last refresh's listing, else the run's structureSearch skeleton pages.
function previousListing(out, events, keep) {
  if (existsSync(join(out, LISTING))) return readJson(join(out, LISTING));
  const start = events.findLastIndex((e) => e.type === 'run');
  const pages = events.slice(Math.max(0, start)).filter((e) => e.type === 'skeleton');
  return [...new Set(pages.flatMap((e) => e.files))].filter((p) => !p.startsWith('..')).filter(keep);
}

// Everything the last build left in <out>: the whole graph, its chunks with their own data, the listing, the options.
function loadResult(out, opts) {
  if (!existsSync(join(out, 'data.json'))) throw new Error(`no data.json in ${out}: build the result before refreshing it`);
  const D = readJson(join(out, 'data.json'));
  const events = existsSync(join(out, 'events.jsonl')) ? readEvents(out) : [];
  const run = events.findLast((e) => e.type === 'run');
  const options = { ...defaults(), ...run?.options, ...opts };
  const plan = D.chunks ?? events.findLast((e) => e.type === 'plan')?.chunks ?? [];
  const chunks = withExcludes(plan.map(({ id, path, files, loose, package: pkg }) => ({ id, path, files, loose, package: pkg })));
  const datas = new Map();
  for (const c of plan.filter((x) => x.built !== false)) {
    const file = join(out, 'chunks', c.id, 'data.json');
    if (existsSync(file)) datas.set(c.id, readJson(file));
  }
  const keep = excludedBy(new Set(options.exclude));
  return { D, chunks, datas, options, keep, files: previousListing(out, events, keep) };
}

// Root-relative files one chunk's snapshot reports changed, added or missing; `reasons` says why a chunk is
// rebuilt without a file list (an older analysis, a frozen copy, or a failed query).
async function staleOf(c, data) {
  const { id, root } = data.snapshot;
  if (data.frozen) return { files: [], reason: 'built from a frozen copy, whose snapshot cannot see later edits' };
  try {
    const r = await readGraph({ operation: 'stale', path: root, graph: id, limit: 1000 });
    const rows = r.rows.filter((row) => typeof (row.file ?? row.path) === 'string');
    const files = rows.map((row) => ({ file: prefixOf(c.path) + (row.file ?? row.path), status: row.status ?? 'changed' }));
    return { files, reason: r.freshness?.analysisCurrent === false ? 'the graph analysis changed since the snapshot' : '' };
  } catch (error) {
    return { files: [], reason: `stale check failed: ${String(error.message).split('\n')[0]}` };
  }
}

// Which chunks hold a change: their snapshot's stale files, plus listed files added or removed inside them.
async function detect(state, { root, emit, jobs }) {
  // The listing's own pages stay out of the stream; a fallback note still reaches the viewer.
  const files = (await listFiles(root, (e) => { if (e.type !== 'skeleton') emit(e); })).filter(state.keep);
  const before = new Set(state.files), now = new Set(files);
  const added = files.filter((p) => !before.has(p)), removed = state.files.filter((p) => !now.has(p));
  const built = state.chunks.filter((c) => state.datas.has(c.id));
  const stale = await pool(built, jobs, (c) => staleOf(c, state.datas.get(c.id)));
  const changed = new Map();
  const mark = (c, file) => { if (c) changed.set(c.id, [...(changed.get(c.id) ?? []), ...(file ? [file] : [])]); };
  built.forEach((c, i) => {
    for (const s of stale[i].files) mark(c, s.file);
    if (stale[i].reason) { mark(c); emit({ type: 'note', level: 'warn', text: `refresh: ${c.path} is rebuilt: ${stale[i].reason}` }); }
  });
  for (const p of [...added, ...removed]) mark(chunkOf(p, state.chunks), p);
  const plan = [...added, ...removed].filter((p) => MANIFESTS.has(basename(p)));
  const editedFiles = [...new Set([...changed.values()].flat())].filter((p) => !added.includes(p) && !removed.includes(p));
  return { files, added, removed, changed, plan, editedFiles };
}

// Files the viewer drops before a chunk's rows arrive again: its graph files, and its listed files that are gone.
function resetFiles(c, state, removed) {
  const data = state.datas.get(c.id);
  const own = (data?.files ?? []).map((f) => prefixOf(c.path) + f.id);
  return [...new Set([...own, ...removed.filter((p) => chunkOf(p, state.chunks) === c)])];
}

// Chunks rebuilt as runPipeline builds them; parts of a chunk refined on scopeTooBroad are built in turn.
// Resolves to the ids of the chunks built (a refined chunk's parts in its place).
async function rebuild(targets, ctx) {
  for (const c of targets) ctx.datas.delete(c.id);
  const built = [];
  const count = { done: 0, total: targets.length };
  let queue = targets;
  while (queue.length) {
    const parts = await pool(queue, ctx.opts.ingestJobs, (c) => buildOne(c, ctx, { built, count }));
    queue = parts.flat();
  }
  return built;
}

async function buildOne(c, ctx, { built, count }) {
  const parts = await buildChunk(c, ctx);
  if (parts.length) count.total += parts.length - 1;
  else { count.done++; built.push(c.id); }
  ctx.emit({ type: 'progress', phase: 'graph', done: count.done, total: count.total });
  return parts;
}

// A changed plan: the viewer drops every chunk, the whole root runs again, and its listing is kept for the next refresh.
async function rerun(state, { root, out, opts, emit, signal }) {
  emit({ type: 'note', level: 'warn', text: 'refresh: a package manifest was added or removed, so the chunk plan changed: rebuilding the whole root' });
  for (const c of state.chunks) emit({ type: 'chunk-reset', id: c.id, files: resetFiles(c, state, []) });
  const listed = [];
  const D = await runPipeline({ root, out, opts, signal, emit: (e) => { if (e.type === 'skeleton') listed.push(...e.files); emit(e); } });
  writeFileSync(join(out, LISTING), JSON.stringify([...new Set(listed)].filter((p) => !p.startsWith('..')).filter(state.keep)));
  return D;
}

// No chunk holds a change: data.json keeps its graph; only files outside every chunk may have come or gone.
function unchanged(state, found, out) {
  if (!found.added.length && !found.removed.length) return state.D;
  state.D.uncoveredFiles = found.files.filter((p) => !chunkOf(p, state.chunks));
  writeFileSync(join(out, 'data.json'), JSON.stringify(state.D));
  writeFileSync(join(out, LISTING), JSON.stringify(found.files));
  return state.D;
}

// The whole result after rebuilding `targets`: stitched again, checked, measured and written.
async function restitch(state, found, ctx, t0) {
  const { root, out, opts, emit } = ctx;
  const phase = (id, s, extra = {}) => emit({ type: 'phase', id, state: s, ms: Date.now() - t0, ...extra });
  for (const c of state.chunks) c.package = manifestName(root, c.path);
  phase('graph', 'start', { total: found.targets.length, note: `refresh: ${found.targets.length} of ${state.chunks.length} chunks` });
  found.rebuilt = await rebuild(found.targets, ctx);
  phase('graph', 'done', { built: ctx.datas.size, total: ctx.chunks.length });
  if (!ctx.datas.size) throw new Error(`No chunk was built: ${ctx.chunks.map((c) => `${c.id}: ${c.error}`).join('; ')}`);
  phase('stitch', 'start');
  const D = stitchPhase(ctx);
  phase('stitch', 'done', { crossImports: D.coverage.crossChunkImports, crossCalls: D.coverage.crossChunkCalls });
  const lsp = opts.lsp === 'on'
    ? { run: true, start: 'refresh: LSP rerun over the whole graph' }
    : { run: false, note: `refresh: LSP not rerun (lsp ${opts.lsp}); calls are graph-only` };
  await finishRun(D, ctx, phase, lsp);
  writeFileSync(join(out, LISTING), JSON.stringify(found.files));
  return D;
}

// Refreshes <out> for the edits made in <root> since it was built; resolves to the new data.json.
// Events: chunk-reset (per rebuilt chunk) -> chunk/rows/progress -> stitched -> verify -> metrics -> refreshed.
export async function refreshRun({ root, out, opts = {}, emit, signal }) {
  const t0 = Date.now();
  reportTools(emit);
  const state = loadResult(out, opts);
  const options = state.options;
  const found = await detect(state, { root, emit, jobs: options.jobs });
  const summary = { changed: found.editedFiles, added: found.added, removed: found.removed };
  if (found.plan.length) {
    const D = await rerun(state, { root, out, opts: options, emit, signal });
    emit({ type: 'refreshed', ms: Date.now() - t0, chunks: ids(D.chunks ?? []), plan: true, ...summary });
    return D;
  }
  found.targets = state.chunks.filter((c) => found.changed.has(c.id));
  if (!found.targets.length) {
    const D = unchanged(state, found, out);
    emit({ type: 'refreshed', ms: Date.now() - t0, chunks: [], ...summary });
    return D;
  }
  if (found.added.length) emit({ type: 'skeleton', files: found.added, page: 1, total: found.files.length });
  for (const c of found.targets) emit({ type: 'chunk-reset', id: c.id, files: resetFiles(c, state, found.removed) });
  // `changed` reaches the wiki (pipeline.mjs finishRun): pages holding these files get their notes marked stale.
  const changed = [...new Set([...found.editedFiles, ...found.added, ...found.removed])];
  const ctx = { root, out, opts: options, emit, signal, files: found.files, chunks: state.chunks, datas: state.datas, used: new Set(ids(state.chunks)), changed };
  const D = await restitch(state, found, ctx, t0);
  emit({ type: 'refreshed', ms: Date.now() - t0, chunks: found.rebuilt, ...summary });
  return D;
}
