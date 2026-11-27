// The whole run as one event stream: list files (structureSearch pages), plan chunks, ingest them in parallel
// (graph pages stream as rows), stitch, cross-check calls with LSP, check files, compute graph metrics, write data.json.
// The live server and the headless build consume the same events; data.json is their fold.
import { checkRunHeaders } from './lib/run-output.ts';
import { writeGraph } from './lib/write-graph.ts';
import { gunzipSync } from 'node:zlib';
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  rmSync,
} from 'node:fs';
import { availableParallelism } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import {
  errorMessage,
  withToolHook,
  prepareGraphRun,
  answerBase,
  nativeWorkspaceRoot,
  engineRootFor,
  withEngineRoot,
} from './lib.ts';
import type { GraphLimits } from './lib.ts';
import { planChunks, refine, name } from './plan.ts';
import { buildSection, CHUNK_DATA } from './section.ts';
import type { SectionEmit, StreamedRows } from './section.ts';
import { chunkDeclarations, chunkOf, packageKey, stitch } from './stitch.ts';
import { graphBuild } from './graph-pages.ts';
import { RunCache, type CacheSettings } from './graph-cache.ts';
import { computeMetrics } from './metrics.ts';
import { describeCall, pagedTool, responseShape, rowsOf } from './tools.ts';
import { writeBrief } from './render.ts';
import {
  lspPhase,
  lspReasons,
  receiverCallSources,
  type CarriedProbes,
  warmLanguageServers,
} from './lsp.ts';
import { readLinks } from './links.ts';
import { recordRun } from './registry.ts';
import {
  workspaceExternals,
  workspaceNote,
  type OutsideImportRow,
} from './workspace-scope.ts';
import type { LspData, LspEvent } from './lsp.ts';
import { verifyPhase } from './verify.ts';
import { measurePackages } from './package-closure.ts';
import { buildWiki } from './wiki.ts';
import type {
  Chunk,
  Emit,
  FileMetrics,
  GraphData,
  Link,
  MeasuredGraph,
  RunEvent,
  RunOptions,
  SectionData,
  UnansweredProbe,
} from './types.ts';
import { prefixOf } from '../ids.ts';

// `jobs`: concurrent tool calls per chunk. The warm server runs graph pages in parallel: on 12 cores one page took
// 0.065 s alone and 0.015 s of wall time each at 12 concurrent calls, so throughput follows the cores.
export const defaults = (): RunOptions => ({
  jobs: availableParallelism(),
  ingestJobs: Math.max(1, Math.floor(availableParallelism() / 4)),
  lsp: 'smart',
  whole: false,
  exclude: [],
});

/** A live viewer's reader of the graph while a run finishes; null once data.json is final. */
export type Publish = (read: (() => MeasuredGraph) | null) => void;

/** A run in progress: its options and stream, the plan, the engine's limits and the chunk data built so far. */
export interface RunContext {
  root: string;
  out: string;
  opts: RunOptions;
  emit: Emit;
  signal?: AbortSignal;
  publish?: Publish;
  files: string[];
  chunks: Chunk[];
  contract: GraphLimits;
  datas: Map<string, SectionData>;
  used: Set<string>;
  /** Files a refresh found changed, added or removed: their wiki pages' notes go stale. */
  changed?: string[];
  /** The links of the graph a refresh replaces: a citation whose cited span changed since then is stale. */
  previousLinks?: Link[];
  /** The shared graph cache (graph-cache.ts): chunk results and LSP answers other runs computed. */
  cache?: RunCache;
}

/** Reports one phase's state (start, done, skipped) with its elapsed time. */
export type Phase = (
  id: string,
  state: string,
  extra?: Record<string, unknown>
) => void;

/** Whether the LSP pass runs, and the note it starts with or is skipped for. */
export interface LspPlan {
  run: boolean;
  note?: string;
  start?: string;
  /** A refresh's previous answers that still hold (lsp.ts carriedProbes). */
  carried?: CarriedProbes;
}

/** The `run` header event: the project, the output directory and the options of a run. */
export interface RunHeaderEvent extends RunEvent {
  type: 'run';
  root: string;
  name: string;
  out: string;
  options: RunOptions;
  startedAt: string;
  /** The engine's workspace root for this run: a refresh started elsewhere roots the engine there (lib.ts). */
  workspace?: string;
}

/** The `plan` event: the chunk plan before any chunk is built. */
export interface PlanEvent extends RunEvent {
  type: 'plan';
  chunks: Pick<Chunk, 'id' | 'path' | 'files' | 'loose' | 'package'>[];
  whole: boolean;
}

/** A `skeleton` event: one page of the file listing. */
export interface SkeletonEvent extends RunEvent {
  type: 'skeleton';
  files: string[];
  page: number;
  total?: number;
}

/** A `phase` or `chunk` event: what a progress line names. */
export interface ProgressEvent extends RunEvent {
  type: 'phase' | 'chunk';
  id: string;
  state: string;
  build?: string;
  error?: string;
}

/** An event read back from events.jsonl: every logged event carries its sequence number. */
export type LoggedRunEvent = RunEvent & { seq: number };

/** A structureSearch `paths` row: a directory and the files listed in it. */
interface StructureGroup {
  dir: string;
  files?: string[];
}

// Tool calls become `tool` events: what ran, which page or cursor, rows, time, bytes, errors.
// Tool ids are unique across every run and refresh that share one events.jsonl: a per-hook run prefix, because the
// call counter restarts in each process.
export function reportTools<T>(emit: Emit, task: () => T) {
  const run = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  return withToolHook(({ id: n, state, args, ms, bytes, error, value }) => {
    const call = describeCall(args);
    const id = `${run}-${n}`;
    if (state === 'start') emit({ type: 'tool', id, state, ...call });
    else
      emit({
        type: 'tool',
        id,
        state: error ? 'error' : 'done',
        ...call,
        ms,
        bytes,
        ...responseShape(value),
        ...(error ? { error } : {}),
      });
  }, task);
}

// File skeleton from structureSearch, page by page; paths come back workspace-relative.
export async function listFiles(root: string, emit: Emit, file?: string) {
  if (file) {
    const files = statSync(join(root, file), {
      throwIfNoEntry: false,
    })?.isFile()
      ? [file]
      : [];
    emit({ type: 'skeleton', files, page: 1, total: files.length });
    return files;
  }
  const files: string[] = [];
  try {
    const base = await nativeWorkspaceRoot();
    await pagedTool(
      'structureSearch',
      {
        queries: [
          { operation: 'files', path: root, entryType: 'file', view: 'paths' },
        ],
      },
      {
        onPage: (page, n) => {
          const from = answerBase(page.results?.[0]?.data, base);
          const rows = rowsOf<StructureGroup>([page], 'files').flatMap(g =>
            (g.files ?? []).map(f => relative(root, resolve(from, g.dir, f)))
          );
          files.push(...rows);
          emit({
            type: 'skeleton',
            files: rows,
            page: n,
            total: page.results?.[0]?.data?.pagination?.total,
          });
        },
      }
    );
  } catch (error) {
    if (
      error instanceof Error &&
      'errorClass' in error &&
      error.errorClass === 'refused'
    )
      throw error;
    // git lists the same tracked and unignored files when structureSearch is unavailable.
    emit({
      type: 'note',
      level: 'warn',
      text: `structureSearch listing failed, using git ls-files: ${errorMessage(error)}`,
    });
    const rows = execFileSync(
      'git',
      ['-C', root, 'ls-files', '--cached', '--others', '--exclude-standard'],
      { encoding: 'utf8', maxBuffer: Infinity }
    )
      .split('\n')
      .filter(p => p && existsSync(join(root, p)));
    files.push(...rows);
    emit({ type: 'skeleton', files: rows, page: 1, total: rows.length });
  }
  return [...new Set(files)].filter(p => !p.startsWith('..'));
}

export function manifestName(root: string, dir: string): string | undefined {
  try {
    const pj = join(root, dir, 'package.json');
    if (existsSync(pj)) return JSON.parse(readFileSync(pj, 'utf8')).name;
    const ct = join(root, dir, 'Cargo.toml');
    if (existsSync(ct))
      return readFileSync(ct, 'utf8').match(/^name\s*=\s*"([^"]+)"/m)?.[1];
  } catch {
    /* a chunk without a readable manifest has no package name */
  }
  return undefined;
}

// Graph rows with ids relative to the repository root, so every chunk's pages share one id space.
// A section streams the rows of the operation `kind` names (section.ts StreamedRows).
function rebase(kind: string, rows: unknown[], prefix: string) {
  const p = <T>(id: T) =>
    typeof id === 'string' && !id.startsWith('pkg:') ? prefix + id : id;
  if (kind === 'files')
    return (rows as StreamedRows['files']).map(n => ({
      id: p(n.id),
      language: n.language,
      roles: n.roles ?? [],
    }));
  if (kind === 'symbols')
    return (rows as StreamedRows['symbols']).map(x => ({
      id: p(x.id),
      file: p(x.path),
      name: x.symbolName,
      kind: x.kind,
      line: x.line,
      endLine: x.endLine,
      exported: !!x.exported,
    }));
  return (rows as StreamedRows['witnesses']).map(w => ({
    edge: w.edge,
    from: p(w.from),
    to: p(w.to),
    file: p(w.path),
    line: w.line,
    confidence: w.confidence,
    detail: w.detail,
    test: !!w.test,
  }));
}

// The listed files a chunk owns, relative to its section (a file section: the file's name).
function chunkFiles(c: Chunk, ctx: RunContext) {
  if (ctx.opts.file) return [ctx.opts.file.split('/').pop()!];
  const prefix = prefixOf(c.path);
  return ctx.files
    .filter(p => chunkOf(p, ctx.chunks)?.id === c.id)
    .map(p => p.slice(prefix.length));
}

// A chunk answered from the graph cache: graphBuild publishes (or reuses) its snapshot, so the freshness checks and a
// later refresh query a live one, and the rows stream once per kind as a replay reads them. Null when the engine's
// snapshot disagrees with the cached one, or the tree changed during the build: the chunk then builds in full.
async function fromCache(
  c: Chunk,
  ctx: RunContext,
  probe: Awaited<ReturnType<RunCache['chunk']>>,
  section: string,
  exclude: string[],
  onRows: SectionEmit
) {
  let snap;
  try {
    snap = await graphBuild(
      { path: section, ...(exclude.length ? { exclude } : {}) },
      { onProgress: line => onRows('build', [], { line }) }
    );
  } catch (error) {
    if (/changed during graph analysis/.test(errorMessage(error))) return null;
    throw error;
  }
  const d = await probe.restore(join(ctx.out, 'chunks', c.id), snap);
  if (!d) return null;
  onRows('snapshot', [], {
    id: snap.graph,
    counts: snap.counts,
    linkage: snap.linkage?.ratio,
  });
  for (const kind of ['files', 'witnesses', 'symbols']) {
    const rows = chunkRows(kind, d, prefixOf(c.path));
    ctx.emit({
      type: 'rows',
      chunk: c.id,
      kind,
      rows,
      page: 1,
      total: rows.length,
    });
  }
  return d;
}

// One chunk's snapshot and graph pages; `ctx.datas` receives its data. A chunk the graph refuses as too broad
// is refined into parts that replace it in `ctx.chunks` and are returned for building. An unchanged chunk another
// run built comes from the graph cache without paging.
export async function buildChunk(c: Chunk, ctx: RunContext): Promise<Chunk[]> {
  const { root, out, opts, emit } = ctx;
  const t0 = Date.now();
  emit({ type: 'chunk', id: c.id, state: 'ingesting' });
  const onRows: SectionEmit = (kind, rows, page) => {
    if (kind === 'snapshot')
      emit({ type: 'chunk', id: c.id, state: 'paging', snapshot: page });
    else if (kind === 'build')
      emit({ type: 'chunk', id: c.id, state: 'ingesting', build: page.line });
    else
      emit({
        type: 'rows',
        chunk: c.id,
        kind,
        rows: rebase(kind, rows, prefixOf(c.path)),
        page: page.page,
        total: page.total,
      });
  };
  try {
    const others = ctx.chunks.filter(x => x !== c);
    const crossPackages = others.length
      ? new Set(others.filter(x => x.package).map(x => packageKey(x.package)))
      : null;
    const section = join(root, opts.file ?? c.path);
    const exclude = [...opts.exclude, ...(c.exclude ?? [])];
    const probe = await ctx.cache?.chunk({
      id: c.id,
      section,
      sectionDir: opts.file ? dirname(section) : section,
      files: chunkFiles(c, ctx),
      exclude,
      file: opts.file ?? null,
      cross: crossPackages !== null,
      extensions: ctx.contract.extensions,
    });
    const cached = probe?.hit
      ? await fromCache(c, ctx, probe, section, exclude, onRows)
      : null;
    const d =
      cached ??
      (await buildSection(section, {
        out: join(out, 'chunks', c.id),
        ...ctx.contract,
        jobs: opts.jobs,
        exclude,
        emit: onRows,
        crossPackages,
      }));
    if (!cached) await probe?.store(join(out, 'chunks', c.id), d);
    ctx.datas.set(c.id, d);
    emit({
      type: 'chunk',
      id: c.id,
      state: 'done',
      ms: Date.now() - t0,
      counts: d.coverage,
      ...(probe && { cache: cached ? 'hit' : probe.key ? 'miss' : 'off' }),
    });
    return [];
  } catch (error) {
    const message = errorMessage(error);
    const refined = /scopeTooBroad/.test(message)
      ? refine(c, ctx.files, ctx.contract.extensions)
      : null;
    if (!refined) {
      c.error = message.split('\n')[0];
      emit({
        type: 'chunk',
        id: c.id,
        state: 'error',
        ms: Date.now() - t0,
        error: c.error,
      });
      return [];
    }
    const parts = name(refined, ctx.used);
    for (const p of parts) p.package = manifestName(root, p.path);
    ctx.chunks.splice(ctx.chunks.indexOf(c), 1, ...parts);
    emit({
      type: 'chunk',
      id: c.id,
      state: 'split',
      ms: Date.now() - t0,
      into: parts.map(({ id, path, files, loose }) => ({
        id,
        path,
        files,
        loose,
      })),
    });
    return parts;
  }
}

// A dynamic queue, largest first. Chunk builds overlap (the engine's workspace lock covers only publication,
// ast_graph/store/build_lock.rs), so the run takes as long as its longest slot: one slot always holds the largest
// remaining chunk, which bounds the total, and the other slots take the smallest so the viewer fills in early.
// Refined parts join the queue while the other chunks keep building.
const bySize = (a: Chunk, b: Chunk) => b.files - a.files;
async function buildAll(ctx: RunContext) {
  const queue = [...ctx.chunks].toSorted(bySize);
  const running = new Set<Chunk>();
  let done = 0;
  await new Promise<void>(finish => {
    const next = () => {
      if (ctx.signal?.aborted) queue.length = 0;
      if (!queue.length && !running.size) return finish();
      while (running.size < ctx.opts.ingestJobs && queue.length) {
        const holdsLargest = [...running].some(r => r.files >= queue[0].files);
        // The loop runs only while the queue holds a chunk.
        const c = (holdsLargest ? queue.pop() : queue.shift())!;
        running.add(c);
        void buildChunk(c, ctx).then(parts => {
          queue.push(...parts);
          queue.sort(bySize);
          running.delete(c);
          done += parts.length ? 0 : 1;
          ctx.emit({
            type: 'progress',
            phase: 'graph',
            done,
            total: ctx.chunks.length,
          });
          next();
        });
      }
    };
    next();
  });
}

// Every node and edge carries where it lives: repository-relative path, absolute path and lines.
function packageManifests(
  D: GraphData,
  chunks: readonly Chunk[],
  root: string
) {
  for (const f of D.files.filter(x => x.kind === 'package')) {
    const owner = chunks.find(c => c.id === f.chunk);
    const dir = owner?.path ?? '.';
    f.manifest = ['package.json', 'Cargo.toml']
      .map(m => (dir === '.' ? m : `${dir}/${m}`))
      .find(m => existsSync(join(root, m)));
  }
}

// A root inside a workspace imports the workspace's other packages from outside (package nodes, or imports left
// unresolved out of the root): the build says which, how many imports each had, and the build of the workspace root
// that stitches them (workspace-scope.ts).
function scopeNotes(
  D: GraphData,
  unresolved: OutsideImportRow[],
  { root, opts, emit }: RunContext
) {
  const scoped = workspaceExternals(
    root,
    { externals: D.externals ?? [], unresolved },
    {
      ...(opts.lsp !== defaults().lsp && { lsp: opts.lsp }),
      exclude: opts.exclude,
    }
  );
  if (!scoped.length) return;
  D.coverage.workspaceExternals = scoped;
  for (const w of scoped)
    emit({ type: 'note', level: 'warn', text: workspaceNote(w) });
}

export function stitchPhase(ctx: RunContext) {
  const { root, chunks, datas, files, emit } = ctx;
  const s = stitch({ root, chunks, datas, files });
  const D = s.whole;
  if (ctx.opts?.file) D.section = join(root, ctx.opts.file);
  scopeNotes(D, s.unresolved, ctx);
  // One chunk: keep its section-wide health, which stitching cannot merge across chunks.
  if (datas.size === 1)
    D.insights.health = [...datas.values()][0].insights.health;
  D.uncoveredFiles = files.filter(p => !chunkOf(p, chunks));
  D.chunks = chunks.map(
    ({ id, path, files: n, loose, exclude, package: pkg, error }) => ({
      id,
      path,
      files: n,
      loose,
      exclude,
      package: pkg,
      built: datas.has(id),
      error,
    })
  );
  packageManifests(D, chunks, root);
  emit({
    type: 'stitched',
    imports: D.imports.filter(e => e.crossChunk),
    calls: D.calls.filter(e => e.crossChunk),
    packages: D.files
      .filter(f => f.kind === 'package')
      .map(({ id, label, chunk, manifest }) => ({
        id,
        label,
        chunk,
        manifest,
      })),
    unresolved: s.unresolved,
    uncovered: D.uncoveredFiles,
  });
  return D;
}

// The checkpoint answers immediately. Live queries derive a new index only after the graph changes.
export async function finishRun(
  D: GraphData,
  ctx: RunContext,
  phase: Phase,
  lsp: LspPlan
) {
  const { root, out, opts, emit, signal, publish } = ctx;
  let status = 'pending';
  if (signal?.aborted) status = 'cancelled';
  else if (!lsp.run) status = 'off';
  else if (ctx.contract.lspBatch === null) status = 'lspSearch unavailable';
  D.callStats = {
    mode: 'graph',
    lsp: status,
    probed: 0,
    resolved: 0,
    both: 0,
    graphOnly: 0,
    lspOnly: 0,
  };
  let current: GraphData | null | undefined;
  function readLive() {
    if (!current) {
      current = { ...D };
      computeMetrics(current);
    }
    // computeMetrics measured the copy.
    return current as MeasuredGraph;
  }
  const checkpoint = join(out, 'stitched.json');
  writeGraph(checkpoint, readLive());
  publish?.(readLive);
  try {
    emit({
      ...metricsEvent(readLive()),
      type: 'graph-ready',
      data: checkpoint,
    });
    if (status !== 'pending') {
      phase('lsp', 'skipped', { note: status === 'off' ? lsp.note : status });
    } else {
      phase('lsp', 'start', lsp.start ? { note: lsp.start } : {});
      const unanswered: UnansweredProbe[] = [];
      // A pending pass has lspSearch's batch limit (an unavailable tool is not pending).
      D.callStats = await lspPhase(D, {
        root,
        signal,
        mode: opts.lsp,
        declarations: chunkDeclarations(ctx.chunks, ctx.datas),
        receiverSources: receiverSourcesOf(ctx),
        carried: lsp.carried,
        cache: await ctx.cache?.probes(root),
        jobs: opts.jobs,
        batchSize: ctx.contract.lspBatch!,
        emit(event) {
          if (event.type === 'lsp') {
            // lspPhase's `lsp` events.
            const batch = event as LspEvent;
            unanswered.push(...batch.unansweredAdded);
            D.callStats = { ...batch.stats, unanswered };
            D.coverage.calls = D.calls.length;
            current = null;
          }
          emit(event);
        },
      });
      current = null;
      phase(
        'lsp',
        D.callStats.probed || cachedAnswers(D) ? 'done' : 'skipped',
        {
          note: D.callStats.lsp,
          lspState: D.callStats.lsp,
        }
      );
    }
    return await finishVerified(D, ctx, phase);
  } finally {
    publish?.(null);
  }
}

// Declarations the LSP pass answered from the graph cache (lsp.ts `cached`).
const cachedAnswers = (D: GraphData) =>
  (D.callStats as { cached?: number } | undefined)?.cached ?? 0;

const CALLABLE_OR_MEMBER = new Set([
  'function',
  'method',
  'constructor',
  'macro',
  'module',
  'label',
  'property',
  'impl',
]);

// The functions holding a receiver call no rule bound to a method some chunk declares (lsp.ts receiverCallSources),
// as root-relative ids.
function receiverSourcesOf(ctx: RunContext) {
  const all = [...chunkDeclarations(ctx.chunks, ctx.datas)];
  const declared = new Set(
    all
      .filter(d => d.kind === 'method' || d.kind === 'function')
      .map(d => d.name)
  );
  // Type names some chunk declares: a receiver typed by anything else is a std or external type.
  const types = new Set(
    all.filter(d => !CALLABLE_OR_MEMBER.has(d.kind)).map(d => d.name)
  );
  const rows = ctx.chunks.flatMap(c => {
    return (ctx.datas.get(c.id)?.unresolvedCalls ?? []).map(u => ({
      from: prefixOf(c.path) + u.from,
      targetName: u.targetName,
      receiverType: u.receiverType,
    }));
  });
  return receiverCallSources(rows, declared, types);
}

async function finishVerified(D: GraphData, ctx: RunContext, phase: Phase) {
  const { root, out, opts, emit } = ctx;
  phase('verify', 'start', { total: D.files.length });
  const checked = await verifyPhase(D, {
    root,
    chunks: ctx.chunks,
    datas: ctx.datas,
    emit,
    jobs: opts.jobs,
    graphBatch: ctx.contract.graphBatch,
  });
  phase('verify', 'done', {
    note: `${checked.ok}/${checked.checked} match their files${checked.unverified ? `, ${checked.unverified} freshness unverified` : ''}${checked.stale ? `, ${checked.stale} stale` : ''}${checked.missing ? `, ${checked.missing} missing` : ''}${checked.lines ? `, ${checked.lines} with lines outside the file` : ''}`,
  });
  phase('metrics', 'start');
  // Links beside the code graph (links.ts); the metrics never count them.
  const linked = await readLinks(
    D,
    root,
    ctx.files,
    ctx.previousLinks ?? [],
    out
  );
  D.links = linked.links;
  D.unresolvedLinks = linked.unresolved;
  // Each external package's bytes: a found bundle's, else its installed closure (package-closure.ts).
  D.externals = measurePackages(D, {
    root,
    bases: ctx.chunks.map(c => join(root, c.path)),
    cache: ctx.cache?.settings.enabled ?? true,
    ...(ctx.cache && { home: ctx.cache.home }),
  });
  computeMetrics(D);
  // computeMetrics measured D.
  const M = D as MeasuredGraph;
  emit(metricsEvent(M));
  writeGraph(join(out, 'data.json'), M);
  // data.json supersedes the checkpoint queries answered from while LSP and the checks ran.
  rmSync(join(out, 'stitched.json'), { force: true });
  const briefs = writeBrief(M, out);
  phase('metrics', 'done', {
    note: `${M.metrics.stats.nodes} nodes, ${M.metrics.stats.edges} weighted edges in ${Object.values(M.metrics.timings).reduce((a, x) => a + x, 0)} ms`,
  });
  wikiPhase(M, ctx);
  return briefs;
}

// The code wiki (wiki.ts) follows the new communities: pages keep their slugs and notes, a refresh passes the files
// it found changed (`ctx.changed`) so their pages' notes go stale. A wiki failure never fails the run.
function wikiPhase(D: GraphData, { out, emit, changed }: RunContext) {
  try {
    const { pages, updated, created, removed } = buildWiki({ D, out, changed });
    emit({ type: 'wiki', pages, updated, created, removed });
  } catch (error) {
    emit({
      type: 'note',
      level: 'warn',
      text: `wiki not updated: ${errorMessage(error).split('\n')[0]}`,
    });
  }
}

export interface PipelineOptions {
  root: string;
  out: string;
  opts: RunOptions;
  emit: Emit;
  signal?: AbortSignal;
  publish?: Publish;
  /** The shared graph cache: on unless `enabled` is false (`--no-cache`). */
  cache?: CacheSettings;
}

// The engine root is settled before any event: the `run` header must stay the log's first line, and the call that
// reads the workspace setting would otherwise be logged ahead of it. A CLI that cannot answer it keeps the default
// root: the run's own contract check (prepareGraphRun) then reports the real cause after the header.
export async function runPipeline(options: PipelineOptions) {
  const engineRoot = await engineRootFor(options.root).catch(() => null);
  return withEngineRoot(engineRoot, async () => {
    const workspace = await nativeWorkspaceRoot().catch(() => undefined);
    return reportTools(options.emit, () =>
      buildPipeline({ ...options, workspace })
    );
  });
}

async function buildPipeline({
  root,
  out,
  opts,
  emit,
  signal,
  publish,
  workspace,
  cache = { enabled: true },
}: PipelineOptions & { workspace: string | undefined }) {
  const t0 = Date.now();
  const phase: Phase = (id, state, extra = {}) =>
    emit({ type: 'phase', id, state, ms: Date.now() - t0, ...extra });
  mkdirSync(out, { recursive: true });
  emit({
    type: 'run',
    root,
    name: opts.file ?? root.split('/').pop(),
    out,
    options: opts,
    startedAt: new Date().toISOString(),
    workspace,
  });
  // The per-user registry finds this result for `query <root>` wherever --out put it (registry.ts).
  recordRun(root, out, text => emit({ type: 'note', level: 'warn', text }));
  const contract = await prepareGraphRun();
  phase('list', 'start');
  // --exclude prunes directory names everywhere, as ingest does, so excluded trees are never planned.
  const excluded = new Set(opts.exclude);
  const files = (await listFiles(root, emit, opts.file)).filter(
    p =>
      !p
        .split('/')
        .slice(0, -1)
        .some(d => excluded.has(d))
  );
  phase('list', 'done', { files: files.length });
  phase('plan', 'start');
  const chunks = planChunks(files, {
    whole: opts.whole,
    extensions: contract.extensions,
  });
  if (opts.file && !chunks.length)
    throw new Error(
      `No graph source file found for ${join(root, opts.file)}. Check that the file exists and its extension has a graph grammar.`
    );
  for (const c of chunks) c.package = manifestName(root, c.path);
  emit({
    type: 'plan',
    chunks: chunks.map(({ id, path, files: n, loose, package: pkg }) => ({
      id,
      path,
      files: n,
      loose,
      package: pkg,
    })),
    whole: opts.whole,
  });
  phase('plan', 'done', { chunks: chunks.length });
  const ctx: RunContext = {
    root,
    out,
    opts,
    emit,
    signal,
    publish,
    files,
    chunks,
    contract,
    datas: new Map(),
    used: new Set(chunks.map(c => c.id)),
    cache: new RunCache(cache, emit),
  };
  // The language servers start while the graph is read; the LSP phase then finds them ready.
  if (opts.lsp !== 'off' && contract.lspBatch !== null && !signal?.aborted)
    void warmLanguageServers(
      root,
      chunks.map(c => c.path),
      files
    );
  phase('graph', 'start', { total: chunks.length });
  await buildAll(ctx);
  phase('graph', 'done', { built: ctx.datas.size, total: chunks.length });
  if (chunks.length && !ctx.datas.size)
    throw new Error(
      `No chunk was built: ${chunks.map(c => `${c.id}: ${c.error}`).join('; ')}`
    );
  phase('stitch', 'start');
  const D = stitchPhase(ctx);
  phase('stitch', 'done', {
    crossImports: D.coverage.crossChunkImports,
    crossCalls: D.coverage.crossChunkCalls,
  });
  const briefs = await finishRun(D, ctx, phase, {
    run: opts.lsp !== 'off',
    note: '--lsp off',
  });
  emit({
    type: 'done',
    ms: Date.now() - t0,
    data: join(out, 'data.json'),
    brief: briefs.brief,
    coverage: D.coverage,
    cache: ctx.cache!.summary(),
  });
  return D;
}

// Per-file metrics as columns aligned to one id list, weighted edges as index columns: compact for the stream.
const COLUMNS: (keyof FileMetrics)[] = [
  'pagerank',
  'betweenness',
  'inWeight',
  'outWeight',
  'depth',
  'core',
  'community',
  'cycle',
];
export function metricsEvent(D: MeasuredGraph) {
  const ids = D.files.map(f => f.id);
  const at = new Map(ids.map((id, i) => [id, i]));
  const edges = D.fileEdges.filter(e => at.has(e.source) && at.has(e.target));
  const { cycles, communities, packages, stats, timings, weightRule } =
    D.metrics;
  return {
    type: 'metrics',
    ids,
    weightRule,
    stats,
    timings,
    cycles,
    packages,
    // computeMetrics measured every file.
    columns: Object.fromEntries(
      COLUMNS.map(k => [k, D.files.map(f => f.metrics![k])])
    ),
    edges: {
      source: edges.map(e => at.get(e.source)),
      target: edges.map(e => at.get(e.target)),
      weight: edges.map(e => e.weight),
      test: edges.map(e => (e.test ? 1 : 0)),
    },
    communities: communities.filter(c => c.size > 1),
    coverage: { ...D.coverage },
    callStats: D.callStats,
    verification: D.verification,
  };
}

// On-demand AST outline of one file (astSearch symbols, every page).
export async function astOutline(root: string, file: string) {
  const pages = await pagedTool('astSearch', {
    queries: [{ operation: 'symbols', path: join(root, file) }],
  });
  const error = pages
    .flatMap(p => p.results ?? [])
    .find(r => r?.data?.errorCode)?.data;
  return {
    symbols: rowsOf(pages, 'symbols'),
    error: error ? `${error.errorCode}: ${error.error ?? ''}` : undefined,
  };
}

/** A viewer's question about one symbol. */
export interface LspQuestion {
  file: string;
  symbolName: string;
  line?: number;
  operation: string;
}

// On-demand LSP question about one symbol: callers, callees, references or definition, every page.
export async function lspQuery(
  root: string,
  { file, symbolName, line, operation }: LspQuestion
) {
  const base = await nativeWorkspaceRoot();
  const pages = await pagedTool('lspSearch', {
    queries: [{ operation, path: join(root, file), symbolName, line }],
  });
  const rows = rowsOf<NonNullable<LspData['files']>[number]>(
    pages,
    'files'
  ).flatMap(g =>
    (g.matches ?? []).map(m => ({
      path: relative(root, resolve(base, m.path ?? g.path)),
      symbolName: m.symbolName,
      kind: m.kind,
      line: m.line,
      endLine: m.endLine,
      sites: m.sites,
    }))
  );
  const error = pages
    .flatMap(p => p.results ?? [])
    .find(r => r?.data?.errorCode)?.data;
  const notes = [
    ...new Set(
      pages.flatMap(p =>
        (p.results ?? []).flatMap(({ data }) => {
          // lspSearch row data.
          const lsp = data as LspData;
          return [
            ...lspReasons(lsp),
            ...(lsp.reason ? [lsp.reason] : []),
            ...(lsp.warnings ?? []),
          ];
        })
      )
    ),
  ];
  return {
    rows,
    notes,
    error: error ? `${error.errorCode}: ${error.error ?? ''}` : undefined,
  };
}

// Saved events of a finished run, for replay without recomputing.
/**
 * One event as events.jsonl stores it. A `rows` event keeps its chunk, kind, page and row count but not the rows:
 * its chunk's result holds them (chunks/<id>/data.json.gz), and readEvents puts them back, so the log holds the
 * graph's rows once (measured on 3,516 Rust files: 43 of 51 MB of the log were rows).
 */
export function logLine(e: RunEvent) {
  if (e.type !== 'rows' || !Array.isArray(e.rows)) return JSON.stringify(e);
  const { rows, ...rest } = e;
  return JSON.stringify({ ...rest, count: rows.length });
}

// A chunk's rows of one kind as its result holds them, in the shape the `rows` event streamed (root-relative).
function chunkRows(kind: string, d: SectionData, prefix: string) {
  const p = (id: string) => (id.startsWith('pkg:') ? id : prefix + id);
  if (kind === 'files')
    return d.files.map(f => ({
      id: p(f.id),
      language: f.language,
      roles: f.roles ?? [],
    }));
  if (kind === 'symbols')
    return (d.declarations ?? []).map(x => ({
      id: p(x.id),
      file: p(x.file),
      name: x.name,
      kind: x.kind,
      line: x.line,
      endLine: x.endLine,
      exported: x.exported,
    }));
  return (['imports', 'calls'] as const).flatMap(edge =>
    (d[edge] ?? []).flatMap(x =>
      (x.lines?.length ? x.lines : [x.line]).map(line => ({
        edge,
        from: p(x.source),
        to: p(x.target),
        file: p(x.source.split('#')[0]),
        line,
        confidence: x.confidence,
        test: !!x.test,
      }))
    )
  );
}

/**
 * The run's events in order. `rows` events come back with their rows, read from the chunk results: the first page
 * of a chunk and kind carries all of them, its later pages none (a replay shows each chunk's current rows).
 * `{ rows: false }` skips that for readers of the headers and listing only.
 */
export function readEvents(dir: string, { rows = true } = {}) {
  const file = join(dir, 'events.jsonl');
  if (!existsSync(file))
    throw new Error(
      `no events.jsonl in ${dir} (${readdirSync(dir).join(', ')})`
    );
  const events: LoggedRunEvent[] = readFileSync(file, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map(l => JSON.parse(l));
  events.forEach(checkRunHeaders(dir));
  if (!rows) return events;
  const paths = new Map<string, string>();
  const results = new Map<string, SectionData | null>();
  const given = new Set<string>();
  const resultOf = (id: string) => {
    if (!results.has(id)) {
      const gz = join(dir, 'chunks', id, CHUNK_DATA);
      results.set(
        id,
        existsSync(gz)
          ? JSON.parse(gunzipSync(readFileSync(gz)).toString('utf8'))
          : null
      );
    }
    return results.get(id)!;
  };
  for (const e of events) {
    if (e.type === 'plan' && Array.isArray(e.chunks))
      for (const c of e.chunks as { id: string; path: string }[])
        paths.set(c.id, c.path);
    if (e.type === 'chunk-reset' && typeof e.id === 'string')
      for (const k of given) if (k.startsWith(`${e.id}\0`)) given.delete(k);
    if (e.type !== 'rows' || Array.isArray(e.rows)) continue;
    const chunk = String(e.chunk);
    const key = `${chunk}\0${String(e.kind)}`;
    const d = given.has(key) ? null : resultOf(chunk);
    given.add(key);
    const path = paths.get(chunk);
    e.rows =
      d && path !== undefined
        ? chunkRows(String(e.kind), d, prefixOf(path))
        : [];
  }
  return events;
}
