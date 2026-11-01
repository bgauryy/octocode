#!/usr/bin/env node
// Headless run of the same pipeline the viewer streams: data.json, events.jsonl and briefs, no browser.
import { appendFileSync, mkdirSync, mkdtempSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { flags } from './lib.mjs';
import { defaults, runPipeline } from './pipeline.mjs';
import { lintWiki } from './wiki-lint.mjs';

const USAGE = `usage: build.mjs <root> [--out <dir>] [--whole] [--lsp on|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--progress]
  Lists files, chunks <root> at package.json/Cargo.toml boundaries (--whole: one snapshot; a chunk the graph
  refuses as scopeTooBroad splits further), ingests chunks in parallel, stitches them, cross-checks calls with
  LSP, and writes <out>/data.json, events.jsonl, brief.txt and brief-full.txt. --progress prints phase lines.`;

const argv = process.argv.slice(2);
if (argv.includes('--help')) { console.log(USAGE); process.exit(0); }
const f = flags(argv, { out: '', lsp: 'on', jobs: '', 'ingest-jobs': '', exclude: '' });
const root = argv[0] && !argv[0].startsWith('-') ? resolve(argv[0]) : null;
if (!root) { console.error(USAGE); process.exit(2); }
const opts = { ...defaults(), whole: argv.includes('--whole'), lsp: f.lsp, exclude: f.exclude.split(',').filter(Boolean) };
if (f.jobs) opts.jobs = Math.max(1, Number(f.jobs));
if (f['ingest-jobs']) opts.ingestJobs = Math.max(1, Number(f['ingest-jobs']));
const out = f.out ? resolve(f.out) : (() => {
  const parent = resolve('.octocode/octocode-graph-view');
  mkdirSync(parent, { recursive: true });
  return mkdtempSync(join(parent, `${new Date().toISOString().slice(0, 10)}-${basename(root)}-`));
})();
mkdirSync(out, { recursive: true });

const t0 = Date.now();
let seq = 0;
const progress = argv.includes('--progress');
const emit = (event) => {
  appendFileSync(join(out, 'events.jsonl'), JSON.stringify({ seq: seq++, t: Date.now() - t0, ...event }) + '\n');
  if (progress && ['phase', 'chunk'].includes(event.type)) console.error(`${((Date.now() - t0) / 1000).toFixed(1)}s ${event.type} ${event.id} ${event.state}${event.build ? ` ${event.build}` : ''}${event.error ? `: ${event.error}` : ''}`);
};
const D = await runPipeline({ root, out, opts, emit });
console.log(JSON.stringify({ ms: Date.now() - t0, out, files: D.coverage.files, imports: D.coverage.imports, calls: D.coverage.calls, chunks: D.coverage.chunks, built: D.coverage.built, crossChunkImports: D.coverage.crossChunkImports, crossChunkCalls: D.coverage.crossChunkCalls, lsp: D.callStats.lsp, brief: join(out, 'brief.txt'), wiki: wikiTodo() }));

// The wiki pages holding most of the PageRank that have no agent notes yet: the agent writes these next
// (SKILL.md). A wiki failure never fails the run; it is reported instead.
function wikiTodo() {
  try {
    return { notesNeeded: lintWiki({ out, root }).issues.filter((i) => i.kind === 'empty-notes').map((i) => i.slug) };
  } catch (error) {
    return { error: error.message };
  }
}
