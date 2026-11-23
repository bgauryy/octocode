// The saved-graph API, the one implementation behind the CLI commands (build, refresh, query, diff), the MCP
// channel and library callers: build a stitched graph with the native engine, refresh it after edits, open it and
// ask it questions, and compare two of its states.
import { appendFileSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { diffGraphs, type GraphDiff } from './diff.ts';
import { defaults, logLine, readEvents, runPipeline } from './pipeline.ts';
import { editedSince, graphPath, runQuery, type QueryResult } from './query.ts';
import type { QueryData } from './query-index.ts';
import { refreshRun } from './refresh.ts';
import { graphInput, lspMode } from './lib/run-options.ts';
import { beginRun, freshOut } from './lib/run-output.ts';
import { postServer, serverOf, type ServerAnswer } from './lib/served.ts';
import type { RefreshedPayload, RunEvent, RunOptions } from './types.ts';

export interface BuildOptions extends Partial<Omit<RunOptions, 'file'>> {
  /** A new result directory; default: a fresh one under ./.octocode/octocode-graph. */
  out?: string;
  /** Each event as the build writes it to events.jsonl (phases, chunks, tool calls). */
  onEvent?: (event: RunEvent) => void;
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
  const emit = (event: RunEvent) => {
    appendFileSync(
      join(out, 'events.jsonl'),
      `${logLine({ seq: seq++, t: Date.now() - t0, ...event })}\n`
    );
    options.onEvent?.(event);
  };
  const D = await runPipeline({ root, out, opts, emit });
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
  };
}

export interface RefreshOptions {
  lsp?: RunOptions['lsp'];
  onEvent?: (event: RunEvent) => void;
  signal?: AbortSignal;
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
 * (`options` then come from the viewer's run).
 */
export async function refreshGraph(
  dir: string,
  options: RefreshOptions = {}
): Promise<RefreshSummary> {
  const out = resolve(dir);
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
 * .octocode/octocode-graph). One load answers any number of queries.
 */
export function openGraph(path: string): SavedGraph {
  const found = graphPath(path);
  // data.json is this package's own output: the query engine tolerates any missing field.
  const data = JSON.parse(readFileSync(found.path, 'utf8')) as QueryData;
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

/** The graph a run's last refresh replaced, kept as previous.json, opened like any saved graph. */
export function previousGraph(dir: string): SavedGraph {
  const file = join(resolve(dir), 'previous.json');
  if (!statSync(file, { throwIfNoEntry: false })?.isFile())
    throw new Error(
      `no ${file}: refresh the result once (octocode-graph refresh) or compare two graphs`
    );
  return openGraph(file);
}
