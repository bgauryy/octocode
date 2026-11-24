// Incremental refresh of an existing result directory after code edits: each built chunk's snapshot answers
// graphQuery `stale`, a fresh structureSearch listing names added and removed files, and only the chunks holding a
// change are rebuilt (graphBuild reuses unchanged snapshots). Every chunk is then stitched again; the LSP pass probes
// only functions an edit could change, carrying the previous answers of the rest (lsp.ts carriedProbes), and the file
// check and graph metrics run over the whole graph, with the same events a full run emits, so an open viewer changes
// in place. A package manifest added or removed changes the chunk plan: the
// whole root runs again. Stateless: everything comes from <out> and the repository.
import { writeGraph } from './lib/write-graph.ts';
import { runHeader } from './lib/run-output.ts';
import {
  copyFileSync,
  existsSync,
  linkSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { basename, join } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { CHUNK_DATA } from './section.ts';
import { planChunks } from './plan.ts';
import { chunkFreshness } from './verify.ts';
import { engineRootFor, pool, prepareGraphRun, withEngineRoot } from './lib.ts';
import {
  buildChunk,
  defaults,
  finishRun,
  listFiles,
  manifestName,
  readEvents,
  reportTools,
  runPipeline,
  stitchPhase,
} from './pipeline.ts';
import type {
  LoggedRunEvent,
  LspPlan,
  Phase,
  PlanEvent,
  Publish,
  RunContext,
  RunHeaderEvent,
  SkeletonEvent,
} from './pipeline.ts';
import { chunkOf } from './stitch.ts';
import { readLinks } from './links.ts';
import { carriedProbes } from './lsp.ts';
import { recordRun } from './registry.ts';
import { RunCache, type CacheSettings } from './graph-cache.ts';
import type {
  Chunk,
  Emit,
  GraphData,
  RunOptions,
  SectionData,
} from './types.ts';
import { prefixOf } from '../ids.ts';

/** A chunk row as data.json or the `plan` event records it. */
type PlannedRow = Pick<
  Chunk,
  'id' | 'path' | 'files' | 'loose' | 'exclude' | 'package'
> & { built?: boolean };

/** Everything the last build left in <out>, and the graph grammars of the engine that refreshes it. */
interface RefreshState {
  D: GraphData;
  chunks: Chunk[];
  datas: Map<string, SectionData>;
  options: RunOptions;
  keep: (path: string) => boolean;
  files: string[];
  extensions?: Set<string>;
}

/** What changed since the last build: the new listing, its added and removed files, the chunks holding a change. */
interface Changes {
  files: string[];
  added: string[];
  removed: string[];
  changed: Map<string, string[]>;
  plan: string[];
  editedFiles: string[];
  /** Chunks rebuilt for a reason no file names (a config change): any of their files may resolve differently. */
  rebuiltWhole: string[];
  targets?: Chunk[];
  rebuilt?: string[];
}

/** The changes with the chunks to rebuild. */
type Targeted = Changes & { targets: Chunk[] };

const MANIFESTS = new Set(['package.json', 'Cargo.toml']);
const LISTING = 'listing.json';
const readJson = (file: string) => JSON.parse(readFileSync(file, 'utf8'));
const excludedBy = (exclude: Set<string>) => (p: string) =>
  !p
    .split('/')
    .slice(0, -1)
    .some(d => exclude.has(d));
const ids = (chunks: readonly Pick<Chunk, 'id'>[]) => chunks.map(c => c.id);

// A loose chunk excludes, by name, each child directory that holds another chunk (plan.ts planDir and refine),
// which data.json's chunk rows do not record.
function withExcludes(chunks: Chunk[]) {
  return chunks.map(c => {
    if (c.exclude || !c.loose) return { ...c };
    const prefix = prefixOf(c.path);
    const names = new Set(
      chunks
        .filter(o => o !== c && o.path !== '.' && o.path.startsWith(prefix))
        .map(o => o.path.slice(prefix.length).split('/')[0])
    );
    return { ...c, exclude: [...names] };
  });
}

// The listing data.json was built from: the last refresh's listing, else the run's structureSearch skeleton pages.
function previousListing(
  out: string,
  events: LoggedRunEvent[],
  keep: (path: string) => boolean
): string[] {
  if (existsSync(join(out, LISTING))) return readJson(join(out, LISTING));
  const start = events.findLastIndex(e => e.type === 'run');
  const pages = events
    .slice(Math.max(0, start))
    .filter((e): e is LoggedRunEvent & SkeletonEvent => e.type === 'skeleton');
  return [...new Set(pages.flatMap(e => e.files))]
    .filter(p => !p.startsWith('..'))
    .filter(keep);
}

// Everything the last build left in <out>: the whole graph, its chunks with their own data, the listing, the options.
function loadResult(out: string, opts: Partial<RunOptions>): RefreshState {
  if (!existsSync(join(out, 'data.json')))
    throw new Error(
      `no data.json in ${out}: build the result before refreshing it`
    );
  const D: GraphData = readJson(join(out, 'data.json'));
  const events = existsSync(join(out, 'events.jsonl'))
    ? readEvents(out, { rows: false })
    : [];
  const run = events.findLast(
    (e): e is LoggedRunEvent & RunHeaderEvent => e.type === 'run'
  );
  const options: RunOptions = { ...defaults(), ...run?.options, ...opts };
  const plan: PlannedRow[] =
    D.chunks ??
    events.findLast((e): e is LoggedRunEvent & PlanEvent => e.type === 'plan')
      ?.chunks ??
    [];
  const chunks = withExcludes(
    plan.map(({ id, path, files, loose, exclude, package: pkg }) => ({
      id,
      path,
      files,
      loose,
      exclude,
      package: pkg,
    }))
  );
  const datas = new Map<string, SectionData>();
  for (const c of plan.filter(x => x.built !== false)) {
    const file = join(out, 'chunks', c.id, CHUNK_DATA);
    if (existsSync(file))
      datas.set(
        c.id,
        JSON.parse(gunzipSync(readFileSync(file)).toString('utf8'))
      );
  }
  const keep = excludedBy(new Set(options.exclude));
  return {
    D,
    chunks,
    datas,
    options,
    keep,
    files: previousListing(out, events, keep),
  };
}

// Which chunks hold a change: their snapshot's stale files, plus listed files added or removed inside them.
async function detect(
  state: RefreshState,
  {
    root,
    emit,
    jobs,
    graphBatch,
  }: { root: string; emit: Emit; jobs: number; graphBatch: number }
): Promise<Changes> {
  // The listing's own pages stay out of the stream; a fallback note still reaches the viewer.
  const files = (
    await listFiles(
      root,
      e => {
        if (e.type !== 'skeleton') emit(e);
      },
      state.options.file
    )
  ).filter(state.keep);
  const before = new Set(state.files),
    now = new Set(files);
  const added = files.filter(p => !before.has(p)),
    removed = state.files.filter(p => !now.has(p));
  const built = state.chunks.filter(c => state.datas.has(c.id));
  const stale = await chunkFreshness(built, state.datas, { jobs, graphBatch });
  const changed = new Map<string, string[]>();
  const rebuiltWhole: string[] = [];
  const mark = (c: Chunk | null, file?: string) => {
    if (c)
      changed.set(c.id, [
        ...(changed.get(c.id) ?? []),
        ...(file ? [file] : []),
      ]);
  };
  built.forEach(c => {
    // chunkFreshness answers every built chunk.
    const { files: staleFiles, reason } = stale.get(c.id)!;
    for (const s of staleFiles) mark(c, s.file);
    if (reason) {
      mark(c);
      rebuiltWhole.push(c.id);
      emit({
        type: 'note',
        level: 'warn',
        text: `refresh: ${c.path} is rebuilt: ${reason}`,
      });
    }
  });
  for (const p of [...added, ...removed]) mark(chunkOf(p, state.chunks), p);
  const changes = [...added, ...removed];
  const plan = changes.filter(p => MANIFESTS.has(basename(p)));
  if (changes.length && !plan.length) {
    const boundaries = (listing: string[]) =>
      planChunks(listing, {
        whole: state.options.whole,
        extensions: state.extensions,
      }).map(c => [c.path, c.exclude]);
    if (
      JSON.stringify(boundaries(state.files).sort()) !==
      JSON.stringify(boundaries(files).sort())
    )
      plan.push(...changes);
  }
  const editedFiles = [...new Set([...changed.values()].flat())].filter(
    p => !added.includes(p) && !removed.includes(p)
  );
  return { files, added, removed, changed, plan, editedFiles, rebuiltWhole };
}

// Files the viewer drops before a chunk's rows arrive again: its graph files, and its listed files that are gone.
function resetFiles(c: Chunk, state: RefreshState, removed: string[]) {
  const data = state.datas.get(c.id);
  const own = (data?.files ?? []).map(f => prefixOf(c.path) + f.id);
  return [
    ...new Set([
      ...own,
      ...removed.filter(p => chunkOf(p, state.chunks) === c),
    ]),
  ];
}

/** The chunks built so far and the progress count, shared by every build of one rebuild. */
interface RebuildProgress {
  built: string[];
  count: { done: number; total: number };
}

// Chunks rebuilt as runPipeline builds them; parts of a chunk refined on scopeTooBroad are built in turn.
// Resolves to the ids of the chunks built (a refined chunk's parts in its place).
async function rebuild(targets: Chunk[], ctx: RunContext) {
  for (const c of targets) ctx.datas.delete(c.id);
  const built: string[] = [];
  const count = { done: 0, total: targets.length };
  let queue = targets;
  while (queue.length) {
    const parts = await pool(queue, ctx.opts.ingestJobs, c =>
      buildOne(c, ctx, { built, count })
    );
    queue = parts.flat();
  }
  return built;
}

async function buildOne(
  c: Chunk,
  ctx: RunContext,
  { built, count }: RebuildProgress
) {
  const parts = await buildChunk(c, ctx);
  if (parts.length) count.total += parts.length - 1;
  else {
    count.done++;
    built.push(c.id);
  }
  ctx.emit({
    type: 'progress',
    phase: 'graph',
    done: count.done,
    total: count.total,
  });
  return parts;
}

/** A refresh's project, output, options and stream. */
export interface RefreshOptions {
  root: string;
  out: string;
  opts?: Partial<RunOptions>;
  emit: Emit;
  signal?: AbortSignal;
  publish?: Publish;
  /** The shared graph cache (graph-cache.ts): on unless `enabled` is false. */
  cache?: CacheSettings;
}

// A changed plan: the viewer drops every chunk, the whole root runs again, and its listing is kept for the next refresh.
async function rerun(
  state: RefreshState,
  {
    root,
    out,
    opts,
    emit,
    signal,
    publish,
    cache,
  }: RefreshOptions & { opts: RunOptions }
) {
  emit({
    type: 'note',
    level: 'warn',
    text: 'refresh: a package manifest was added or removed, so the chunk plan changed: rebuilding the whole root',
  });
  for (const c of state.chunks)
    emit({ type: 'chunk-reset', id: c.id, files: resetFiles(c, state, []) });
  const listed: string[] = [];
  // A `skeleton` event carries one listing page's files.
  const D = await runPipeline({
    root,
    out,
    opts,
    signal,
    publish,
    cache,
    emit: e => {
      if (e.type === 'skeleton') listed.push(...(e as SkeletonEvent).files);
      emit(e);
    },
  });
  writeFileSync(
    join(out, LISTING),
    JSON.stringify(
      [...new Set(listed)].filter(p => !p.startsWith('..')).filter(state.keep)
    )
  );
  return D;
}

// No chunk holds a change: data.json keeps its graph; only files outside every chunk may have come or gone, and docs
// (never in a chunk) may have changed, so their links are read again. A graph left as it was is still marked checked
// now (its modification time), so files touched without a change no longer read as edited since it was written.
async function unchanged(
  state: RefreshState,
  found: Changes,
  out: string,
  root: string
) {
  const docs = await readLinks(
    state.D,
    root,
    found.files,
    state.D.links ?? [],
    out
  );
  const linksChanged =
    JSON.stringify([docs.links, docs.unresolved]) !==
    JSON.stringify([state.D.links ?? [], state.D.unresolvedLinks ?? []]);
  if (!found.added.length && !found.removed.length && !linksChanged) {
    const now = new Date();
    utimesSync(join(out, 'data.json'), now, now);
    return state.D;
  }
  state.D.links = docs.links;
  state.D.unresolvedLinks = docs.unresolved;
  state.D.uncoveredFiles = found.files.filter(p => !chunkOf(p, state.chunks));
  writeGraph(join(out, 'data.json'), state.D);
  writeFileSync(join(out, LISTING), JSON.stringify(found.files));
  return state.D;
}

// The whole result after rebuilding `targets`: stitched again, checked, measured and written.
async function restitch(
  state: RefreshState,
  found: Targeted,
  ctx: RunContext,
  t0: number
) {
  const { root, out, opts, emit } = ctx;
  const phase: Phase = (id, s, extra = {}) =>
    emit({ type: 'phase', id, state: s, ms: Date.now() - t0, ...extra });
  for (const c of state.chunks) c.package = manifestName(root, c.path);
  phase('graph', 'start', {
    total: found.targets.length,
    note: `refresh: ${found.targets.length} of ${state.chunks.length} chunks`,
  });
  found.rebuilt = await rebuild(found.targets, ctx);
  phase('graph', 'done', { built: ctx.datas.size, total: ctx.chunks.length });
  if (!ctx.datas.size)
    throw new Error(
      `No chunk was built: ${ctx.chunks.map(c => `${c.id}: ${c.error}`).join('; ')}`
    );
  phase('stitch', 'start');
  const D = stitchPhase(ctx);
  phase('stitch', 'done', {
    crossImports: D.coverage.crossChunkImports,
    crossCalls: D.coverage.crossChunkCalls,
  });
  // Answers of the previous run carry over unless an edit could change them (lsp.ts carriedProbes).
  const edited = new Set([
    ...found.editedFiles,
    ...found.added,
    ...found.removed,
  ]);
  const whole = new Set(found.rebuiltWhole);
  const carried = carriedProbes(
    state.D,
    file => edited.has(file) || whole.has(chunkOf(file, state.chunks)?.id ?? '')
  );
  const lsp: LspPlan =
    opts.lsp !== 'off'
      ? {
          run: true,
          start: `refresh: LSP for changed code; ${carried.sources.size} unchanged answers carried over`,
          carried,
        }
      : {
          run: false,
          note: `refresh: LSP not rerun (lsp ${opts.lsp}); calls are graph-only`,
        };
  await finishRun(D, ctx, phase, lsp);
  writeFileSync(join(out, LISTING), JSON.stringify(found.files));
  return D;
}

// The graph this refresh replaces stays as previous.json, for `octocode-graph diff <out>`. A hard link costs no copy:
// writeGraph renames a new file over data.json, so the old content lives on under the second name.
function keepPrevious(out: string) {
  const current = join(out, 'data.json');
  const previous = join(out, 'previous.json');
  rmSync(previous, { force: true });
  try {
    linkSync(current, previous);
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'EXDEV'))
      throw error;
    copyFileSync(current, previous);
  }
}

// Refreshes <out> for the edits made in <root> since it was built; resolves to the new data.json.
// Events: chunk-reset (per rebuilt chunk) -> chunk/rows/progress -> stitched -> verify -> metrics -> refreshed.
export async function refreshRun(options: RefreshOptions) {
  // As for a build (pipeline.ts runPipeline): an unanswered setting keeps the default root.
  const engineRoot = await engineRootFor(
    options.root,
    recordedWorkspace(options.out)
  ).catch(() => null);
  return withEngineRoot(engineRoot, () =>
    reportTools(options.emit, () => refreshResult(options))
  );
}

// The engine workspace the build recorded in its run header (pipeline.ts RunHeaderEvent), when it did.
function recordedWorkspace(out: string) {
  const workspace = runHeader(out)?.workspace;
  return typeof workspace === 'string' ? workspace : null;
}

async function refreshResult({
  root,
  out,
  opts = {},
  emit,
  signal,
  publish,
  cache = { enabled: true },
}: RefreshOptions) {
  const t0 = Date.now();
  // The per-user registry finds this result for `query <root>` wherever it lives (registry.ts).
  recordRun(root, out, text => emit({ type: 'note', level: 'warn', text }));
  const contract = await prepareGraphRun();
  const state = loadResult(out, opts);
  keepPrevious(out);
  state.extensions = contract.extensions;
  const options = state.options;
  const detected = await detect(state, {
    root,
    emit,
    jobs: options.jobs,
    graphBatch: contract.graphBatch,
  });
  const summary = {
    changed: detected.editedFiles,
    added: detected.added,
    removed: detected.removed,
  };
  if (detected.plan.length) {
    const D = await rerun(state, {
      root,
      out,
      opts: options,
      emit,
      signal,
      publish,
      cache,
    });
    emit({
      type: 'refreshed',
      ms: Date.now() - t0,
      chunks: ids(D.chunks ?? []),
      plan: true,
      ...summary,
    });
    return D;
  }
  const found: Targeted = Object.assign(detected, {
    targets: state.chunks.filter(c => detected.changed.has(c.id)),
  });
  if (!found.targets.length) {
    const D = await unchanged(state, found, out, root);
    emit({ type: 'refreshed', ms: Date.now() - t0, chunks: [], ...summary });
    return D;
  }
  if (found.added.length)
    emit({
      type: 'skeleton',
      files: found.added,
      page: 1,
      total: found.files.length,
    });
  for (const c of found.targets)
    emit({
      type: 'chunk-reset',
      id: c.id,
      files: resetFiles(c, state, found.removed),
    });
  // `changed` reaches the wiki (pipeline.ts finishRun): pages holding these files get their notes marked stale.
  const changed = [
    ...new Set([...found.editedFiles, ...found.added, ...found.removed]),
  ];
  const ctx: RunContext = {
    root,
    out,
    opts: options,
    emit,
    signal,
    publish,
    contract,
    files: found.files,
    chunks: state.chunks,
    datas: state.datas,
    used: new Set(ids(state.chunks)),
    changed,
    previousLinks: state.D.links ?? [],
    cache: new RunCache(cache, emit),
  };
  const D = await restitch(state, found, ctx, t0);
  emit({
    type: 'refreshed',
    ms: Date.now() - t0,
    chunks: found.rebuilt,
    ...summary,
  });
  return D;
}
