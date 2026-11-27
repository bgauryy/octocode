// The saved-graph API, the one implementation behind the CLI commands (build, refresh, query, diff), the MCP
// channel and library callers: build a stitched graph with the native engine, refresh it after edits, open it and
// ask it questions, and compare two of its states.
import { appendFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { diffGraphs, type GraphDiff } from './diff.ts';
import { defaults, logLine, readEvents, runPipeline } from './pipeline.ts';
import {
  editedSince,
  graphPath,
  readGraphFile,
  resultDir,
  runQuery,
  type QueryResult,
} from './query.ts';
import { graphCli, shellArg } from './invocation.ts';
import type { WorkspaceExternals } from './workspace-scope.ts';
import type { QueryData } from './query-index.ts';
import { refreshRun } from './refresh.ts';
import { graphInput, lspMode } from './lib/run-options.ts';
import { beginRun, freshOut } from './lib/run-output.ts';
import { postServer, serverOf, type ServerAnswer } from './lib/served.ts';
import type { RefreshedPayload, RunEvent, RunOptions } from './types.ts';
import type { CacheEvent } from './graph-cache.ts';

export interface BuildOptions extends Partial<Omit<RunOptions, 'file'>> {
  /** A new result directory; default: a fresh one under ./.octocode/octocode-graph. */
  out?: string;
  /** Each event as the build writes it to events.jsonl (phases, chunks, tool calls). */
  onEvent?: (event: RunEvent) => void;
  /** false: neither read nor write the shared graph cache (graph-cache.ts). Default true. */
  cache?: boolean;
}

/** The shared graph cache's hits and misses in one run: chunks, and LSP answers per declaration. */
export interface CacheSummary {
  enabled: boolean;
  chunks: { hit: number; miss: number };
  lsp: { hit: number; miss: number };
}

export interface BuildSummary {
  out: string;
  ms: number;
  files: number;
  imports: number;
  calls: number;
  chunks?: number;
  built?: number;
  crossChunkImports?: number;
  crossChunkCalls?: number;
  lsp: string;
  brief: string;
  /** The enclosing workspaces whose packages this build treated as externals, and the build that stitches them. */
  workspaceExternals?: WorkspaceExternals[];
  /** What the shared graph cache answered (each lookup is a `cache` event in events.jsonl, with why it missed). */
  cache?: CacheSummary;
}

/** Build the stitched graph of a file or directory into a new result directory. */
export async function buildGraph(
  path: string,
  options: BuildOptions = {}
): Promise<BuildSummary> {
  const input = resolve(path);
  const { root, file } = graphInput(input);
  const out = options.out ? resolve(options.out) : freshOut(input);
  const base = defaults();
  const opts: RunOptions = {
    jobs: options.jobs ?? base.jobs,
    ingestJobs: options.ingestJobs ?? base.ingestJobs,
    lsp: lspMode(options.lsp ?? base.lsp),
    whole: options.whole ?? false,
    exclude: options.exclude ?? [],
    ...(file && { file }),
  };
  beginRun(out);
  const t0 = Date.now();
  let seq = 0;
  let cache: CacheSummary | undefined;
  const emit = (event: RunEvent) => {
    appendFileSync(
      join(out, 'events.jsonl'),
      `${logLine({ seq: seq++, t: Date.now() - t0, ...event })}\n`
    );
    // The `done` event carries the cache summary (pipeline.ts).
    if (event.type === 'done') cache = event.cache as CacheSummary | undefined;
    options.onEvent?.(event);
  };
  const D = await runPipeline({
    root,
    out,
    opts,
    emit,
    cache: { enabled: options.cache !== false },
  });
  return {
    out,
    ms: Date.now() - t0,
    files: D.coverage.files,
    imports: D.coverage.imports,
    calls: D.coverage.calls,
    chunks: D.coverage.chunks,
    built: D.coverage.built,
    crossChunkImports: D.coverage.crossChunkImports,
    crossChunkCalls: D.coverage.crossChunkCalls,
    lsp: D.callStats?.lsp ?? 'off',
    brief: join(out, 'brief.txt'),
    ...(D.coverage.workspaceExternals && {
      workspaceExternals: D.coverage.workspaceExternals,
    }),
    ...(cache && { cache }),
  };
}

/** A `cache` event as one line: scope, id, hit or miss, and the first key part that differs from the nearest entry. */
export function explainCacheEvent(e: CacheEvent) {
  const counts =
    e.scope === 'lsp' ? ` (${e.hits ?? 0} hit, ${e.misses ?? 0} miss)` : '';
  return `cache ${e.scope} ${e.id} ${e.state}${counts}${e.reason ? `: ${e.reason.part} ${e.reason.detail}` : ''}`;
}

export interface RefreshOptions {
  lsp?: RunOptions['lsp'];
  onEvent?: (event: RunEvent) => void;
  signal?: AbortSignal;
  /** false: neither read nor write the shared graph cache. Default true. */
  cache?: boolean;
}

/** What a refresh did: its `refreshed` event (rebuilt chunk ids; files changed, added and removed) and its time. */
export interface RefreshSummary extends RefreshedPayload {
  out: string;
  /** The chunk plan changed (a manifest came or went), so every chunk was planned again. */
  plan?: boolean;
}

/**
 * Bring a saved run up to date after edits: only the chunks holding a change are rebuilt, then the whole graph is
 * stitched, checked and measured again. The replaced graph stays as previous.json (`diffGraphs`), and the events
 * append to events.jsonl, continuing its sequence. A run a live viewer serves refreshes through that viewer
 * (`options` then come from the viewer's run). `dir` is a result directory, or a project directory: its newest
 * saved graph (openGraph).
 */
export async function refreshGraph(
  dir: string,
  options: RefreshOptions = {}
): Promise<RefreshSummary> {
  const out = resultDir(dir);
  const t0 = Date.now();
  // A run a live viewer serves is that viewer's to write: it refreshes, and its page follows.
  const viewer = serverOf(out);
  if (viewer) {
    const { ok: _ok, ...summary } = (await postServer(
      viewer,
      '/refresh',
      {}
    )) as ServerAnswer & RefreshSummary;
    return { ...summary, out, ms: Date.now() - t0 };
  }
  const events = readEvents(out, { rows: false });
  const root = events.findLast(e => e.type === 'run')?.root;
  if (typeof root !== 'string')
    throw new Error(`the event log in ${out} names no run root`);
  const last = events.at(-1);
  let seq = (last?.seq ?? -1) + 1;
  // Continue the saved timeline: `t` counts from the original run's start.
  const start = Date.now() - (last?.t ?? 0);
  let refreshed: RunEvent | undefined;
  const emit = (event: RunEvent) => {
    appendFileSync(
      join(out, 'events.jsonl'),
      `${logLine({ seq: seq++, t: Date.now() - start, ...event })}\n`
    );
    if (event.type === 'refreshed') refreshed = event;
    options.onEvent?.(event);
  };
  const opts = options.lsp === undefined ? {} : { lsp: lspMode(options.lsp) };
  await refreshRun({
    root,
    out,
    opts,
    emit,
    ...(options.signal && { signal: options.signal }),
    cache: { enabled: options.cache !== false },
  });
  // refreshRun always ends with its `refreshed` event.
  const { type: _type, ...summary } = refreshed as RunEvent & RefreshSummary;
  return { ...summary, out, ms: Date.now() - t0 };
}

export interface SavedGraph {
  /** The graph file read (data.json, or stitched.json while a run finishes). */
  path: string;
  /** The result directory chosen for a project directory, else null. */
  run: string | null;
  data: QueryData;
  /** Graph files changed or missing since the graph was written: rebuild or refresh before trusting their edges. */
  edited: string[];
  query(text: string): QueryResult;
  /** What changed structurally from `before` (another saved graph, or its path) to this graph. */
  diffFrom(before: SavedGraph | string): GraphDiff;
}

/**
 * Open a saved graph: a result directory, its data.json, or a project directory (its newest saved graph under
 * .octocode/octocode-graph, else the newest the per-user graph registry lists for it). One load answers any number
 * of queries.
 */
export function openGraph(path: string): SavedGraph {
  const found = graphPath(path);
  const data = readGraphFile(found.path);
  return {
    path: found.path,
    run: found.chosen,
    data,
    edited: editedSince(data, found.path),
    query: text => runQuery(data, text),
    diffFrom: before =>
      diffGraphs(
        typeof before === 'string' ? openGraph(before).data : before.data,
        data
      ),
  };
}

/** A saved graph opened after its edits were refreshed, and that refresh (null: nothing had changed). */
export interface FreshGraph extends SavedGraph {
  refreshed: RefreshSummary | null;
}

/**
 * Open a saved graph (as openGraph) up to date: when graph files changed or went missing since it was written, the
 * chunks holding them are refreshed first (refreshGraph) and the refreshed graph is returned; unchanged, it is the
 * graph as read, with no refresh.
 */
export async function openFreshGraph(
  path: string,
  options: RefreshOptions = {}
): Promise<FreshGraph> {
  const opened = openGraph(path);
  if (!opened.edited.length) return { ...opened, refreshed: null };
  const out = opened.run ?? dirname(opened.path);
  const refreshed = await refreshGraph(out, options);
  return { ...openGraph(out), run: opened.run, refreshed };
}

/** The graph a run's last refresh replaced, kept as previous.json, opened like any saved graph. */
export function previousGraph(dir: string): SavedGraph {
  const out = resultDir(dir);
  const file = join(out, 'previous.json');
  if (!statSync(file, { throwIfNoEntry: false })?.isFile())
    throw new Error(
      `no ${file}: refresh the result once (${graphCli(`refresh ${shellArg(out)}`)}) or compare two graphs`
    );
  return openGraph(file);
}
