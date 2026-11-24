#!/usr/bin/env node
// `octocode-graph build`: the headless build (api.ts buildGraph), the same pipeline the viewer streams.
import { join } from 'node:path';
import { buildGraph, explainCacheEvent } from '../api.ts';
import type { CacheEvent } from '../graph-cache.ts';
import { errorMessage } from '../lib.ts';
import { runOptions } from '../lib/run-options.ts';
import type { ProgressEvent } from '../pipeline.ts';
import type { RunEvent } from '../types.ts';
import { graphCommand } from '../invocation.ts';
import { workspaceNote } from '../workspace-scope.ts';

const cli = graphCommand();
const USAGE = `usage: ${cli} build <file-or-directory> [--out <dir>] [--whole] [--lsp smart|all|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--progress] [--no-cache] [--explain-cache]
  Lists the selected file or directory, chunks directories at package.json/Cargo.toml boundaries (--whole: one snapshot; a chunk the graph
  refuses as scopeTooBroad splits further), ingests chunks in parallel, stitches them, cross-checks calls with
  LSP, and writes <out>/data.json, events.jsonl, brief.txt. --progress prints phase lines. The result is registered
  for its root, so ${cli} query <root> finds it wherever --out put it. A root inside a workspace whose packages it
  imports names those packages and the build of the workspace root that stitches them (stderr, brief.txt).
  Unchanged chunks and LSP answers come from the shared graph cache in the Octocode home (${cli} cache stats); --no-cache
  bypasses it, --explain-cache prints each chunk and probe group as hit or miss with the first key part that changed.
  Then: ${cli} query <out> "search <name>"; after edits, ${cli} query <out> "<query>" --refresh or ${cli} refresh <out>.`;

const argv = process.argv.slice(2);
if (argv.includes('--help')) {
  console.log(USAGE);
  process.exit(0);
}
let options;
try {
  options = runOptions(argv);
} catch (error) {
  console.error(errorMessage(error), `\n${USAGE}`);
  process.exit(2);
}
const { root, out, opts, progress, cache } = options;
const isProgress = (event: RunEvent): event is ProgressEvent =>
  ['phase', 'chunk'].includes(event.type);
const t0 = Date.now();
try {
  // A build always has a project root (--replay is a viewer flag).
  const summary = await buildGraph(opts.file ? join(root!, opts.file) : root!, {
    ...opts,
    out,
    cache: cache.enabled,
    onEvent: event => {
      if (cache.explain && event.type === 'cache')
        console.error(explainCacheEvent(event as unknown as CacheEvent));
      if (progress && isProgress(event))
        console.error(
          `${((Date.now() - t0) / 1000).toFixed(1)}s ${event.type} ${event.id} ${event.state}${event.build ? ` ${event.build}` : ''}${event.error ? `: ${event.error}` : ''}`
        );
    },
  });
  console.log(JSON.stringify(summary));
  for (const w of summary.workspaceExternals ?? [])
    console.error(`octocode-graph build: ${workspaceNote(w)}`);
} catch (error) {
  console.error(errorMessage(error));
  process.exit(1);
}
