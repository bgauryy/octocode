#!/usr/bin/env node
// `octocode-graph build`: the headless build (api.ts buildGraph), the same pipeline the viewer streams.
import { join } from 'node:path';
import { buildGraph } from '../api.ts';
import { errorMessage } from '../lib.ts';
import { runOptions } from '../lib/run-options.ts';
import type { ProgressEvent } from '../pipeline.ts';
import type { RunEvent } from '../types.ts';

const USAGE = `usage: octocode-graph build <file-or-directory> [--out <dir>] [--whole] [--lsp smart|all|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--progress]
  Lists the selected file or directory, chunks directories at package.json/Cargo.toml boundaries (--whole: one snapshot; a chunk the graph
  refuses as scopeTooBroad splits further), ingests chunks in parallel, stitches them, cross-checks calls with
  LSP, and writes <out>/data.json, events.jsonl, brief.txt. --progress prints phase lines.
  Then: octocode-graph query <out> "search <name>"; after edits, octocode-graph refresh <out>.`;

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
const { root, out, opts, progress } = options;
const isProgress = (event: RunEvent): event is ProgressEvent =>
  ['phase', 'chunk'].includes(event.type);
const t0 = Date.now();
try {
  // A build always has a project root (--replay is a viewer flag).
  const summary = await buildGraph(opts.file ? join(root!, opts.file) : root!, {
    ...opts,
    out,
    onEvent: event => {
      if (progress && isProgress(event))
        console.error(
          `${((Date.now() - t0) / 1000).toFixed(1)}s ${event.type} ${event.id} ${event.state}${event.build ? ` ${event.build}` : ''}${event.error ? `: ${event.error}` : ''}`
        );
    },
  });
  console.log(JSON.stringify(summary));
} catch (error) {
  console.error(errorMessage(error));
  process.exit(1);
}
