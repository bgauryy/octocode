#!/usr/bin/env node
// Headless run of the same pipeline the viewer streams: data.json, events.jsonl and briefs, no browser.
import { appendFileSync } from 'node:fs';
import { join } from 'node:path';
import { runOptions } from './lib/run-options.mjs';
import { beginRun } from './lib/run-output.mjs';
import { runPipeline } from './pipeline.mjs';

const USAGE = `usage: build.mjs <file-or-directory> [--out <dir>] [--whole] [--lsp smart|all|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--progress]
  Lists the selected file or directory, chunks directories at package.json/Cargo.toml boundaries (--whole: one snapshot; a chunk the graph
  refuses as scopeTooBroad splits further), ingests chunks in parallel, stitches them, cross-checks calls with
  LSP, and writes <out>/data.json, events.jsonl, brief.txt and brief-full.txt. --progress prints phase lines.`;

const argv = process.argv.slice(2);
if (argv.includes('--help')) { console.log(USAGE); process.exit(0); }
let options;
try { options = runOptions(argv); } catch (error) { console.error(error.message, `\n${USAGE}`); process.exit(2); }
const { root, out, opts, progress } = options;
try { beginRun(out); } catch (error) { console.error(error.message); process.exit(1); }

const t0 = Date.now();
let seq = 0;
const emit = (event) => {
  appendFileSync(join(out, 'events.jsonl'), `${JSON.stringify({ seq: seq++, t: Date.now() - t0, ...event })}\n`);
  if (progress && ['phase', 'chunk'].includes(event.type)) console.error(`${((Date.now() - t0) / 1000).toFixed(1)}s ${event.type} ${event.id} ${event.state}${event.build ? ` ${event.build}` : ''}${event.error ? `: ${event.error}` : ''}`);
};
const D = await runPipeline({ root, out, opts, emit });
console.log(JSON.stringify({ ms: Date.now() - t0, out, files: D.coverage.files, imports: D.coverage.imports, calls: D.coverage.calls, chunks: D.coverage.chunks, built: D.coverage.built, crossChunkImports: D.coverage.crossChunkImports, crossChunkCalls: D.coverage.crossChunkCalls, lsp: D.callStats.lsp, brief: join(out, 'brief.txt') }));
