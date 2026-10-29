#!/usr/bin/env node
// Octocode report viewer: render skill Markdown output as one self-contained local HTML page.
// Source: skills/common/scripts/render-report.mjs. Skill copies are generated; do not edit them.
// Regenerate with node skills/common/build.mjs.

import { isMainModule } from './lib/entrypoint.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { buildReport, defaultOut, manifestPage } from './lib/report/render.mjs';
import { selfTest } from './lib/report/self-test.mjs';

const HELP = `Render skill Markdown output as one self-contained HTML report. The page stays closed.

Usage:
  node scripts/render-report.mjs <file.md | folder> [--out <file.html>] [--title <text>] [--open] [--json] [--strict]
  node scripts/render-report.mjs <file.md | folder> --manifest [--query <text>] [--limit <n>] [--offset <n>] [--all]
  node scripts/render-report.mjs --self-test
  node scripts/render-report.mjs --help

A file renders one report. A folder renders every Markdown file under it (skipping tmp/, view/,
node_modules/, and dot folders) as a library; <workspace>/.octocode renders all saved skill output.

Default output, next to the source when it is under <workspace>/.octocode/:
  <name>.md -> <name>.html        <folder> -> <folder>/index.html
Other sources go to <workspace>/.octocode/<this skill>/view/<name>/index.html.

The page renders Markdown locally (GFM tables, task lists, alerts, footnotes, code highlighting),
links code references such as \`src/a.ts:12\` that exist in the workspace to your editor, lists every
cited source, and has search, a contents rail, light/dark themes, and print styles. Mermaid diagrams
load mermaid from a pinned CDN; offline, the diagram source is shown instead. Raw HTML is escaped.
Leaves the page closed. Pass --open only when the reader asked to see it. --no-open is
accepted and keeps the page closed. CI, or Linux with no display, also stays closed.

--manifest writes no HTML. Default: 20 compact records (path/title/status/question), with a
next argument array to reach the rest. --query filters titles, paths and questions. --limit and
--offset select a page. --all explicitly returns the full unabridged catalog for file processing.
--strict rejects validation warnings with exit 2 before writing HTML. JSON render results always
say render: unverified: file generation does not establish browser rendering success. Open the
page and inspect its Render check before claiming the diagrams work.`;

function canOpen() {
  if (process.env.CI) return false;
  if (process.platform === 'linux') return Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  return process.platform === 'darwin' || process.platform === 'win32';
}

function openFile(file) {
  const cmd = process.platform === 'darwin' ? ['open', [file]] : process.platform === 'win32' ? ['cmd', ['/c', 'start', '', file]] : ['xdg-open', [file]];
  try { spawn(cmd[0], cmd[1], { stdio: 'ignore', detached: true }).unref(); return true; } catch { return false; }
}

const VALUE_OPTIONS = new Map([['--out', 'out'], ['--title', 'title'], ['--workspace', 'workspace']]);
const FLAG_OPTIONS = new Map([
  ['--open', ['open', true]], ['--no-open', ['open', false]], ['--json', ['json', true]], ['--manifest', ['manifest', true]],
  ['--strict', ['strict', true]], ['--all', ['all', true]], ['--help', ['help', true]], ['-h', ['help', true]], ['--self-test', ['selfTest', true]],
]);

// Reads the option at argv[i] into args; returns the index of the last argument it consumed.
function readOption(args, argv, i) {
  const a = argv[i];
  if (VALUE_OPTIONS.has(a)) {
    const value = argv[i + 1];
    if (!value || value.startsWith('--') || FLAG_OPTIONS.has(value)) throw new Error(`${a} requires a value`);
    args[VALUE_OPTIONS.get(a)] = value;
    return i + 1;
  }
  if (FLAG_OPTIONS.has(a)) { const [key, value] = FLAG_OPTIONS.get(a); args[key] = value; return i; }
  if (a === '--query') { args.query = argv[i + 1]; if (!args.query || args.query.startsWith('--')) throw new Error('--query requires text'); return i + 1; }
  if (a === '--limit' || a === '--offset') {
    const value = Number(argv[i + 1]);
    if (!Number.isSafeInteger(value) || value < (a === '--limit' ? 1 : 0)) throw new Error(`Invalid ${a}`);
    args[a.slice(2)] = value;
    return i + 1;
  }
  if (a.startsWith('--')) throw new Error(`Unknown option: ${a}`);
  args.positional.push(a);
  return i;
}

function parseArgs(argv) {
  const args = { positional: [], open: false, json: false };
  for (let i = 0; i < argv.length; i++) i = readOption(args, argv, i);
  return args;
}

function render(args) {
  try {
    const report = buildReport(args.positional[0], { title: args.title, workspace: args.workspace });
    const out = path.resolve(args.out || defaultOut(report.abs, report.isDir, report.workspace));
    fs.mkdirSync(path.dirname(out), { recursive: true });
    if (args.strict && report.warnings.length) { console.error(JSON.stringify({validation:'failed',warnings:report.warnings})); return 2; }
    fs.writeFileSync(out, report.html);
    const opened = args.open && canOpen() ? openFile(out) : false;
    for (const w of report.warnings) console.error(`warning: ${w}`);
    if (args.json) console.log(JSON.stringify({ out, validation: report.warnings.length ? 'warnings' : 'passed', render: 'unverified', files: report.files.length, bytes: Buffer.byteLength(report.html), opened, ...(report.warnings.length ? { warnings: report.warnings } : {}) }));
    else console.log(`${out}\n${report.files.length} document${report.files.length === 1 ? '' : 's'}${opened ? ', opened' : ''}`);
    return 0;
  } catch (e) {
    console.error(e.message);
    return 1;
  }
}

function main() {
  let args;
  try { args = parseArgs(process.argv.slice(2)); } catch (e) { console.error(e.message); return 1; }
  if (args.help) { console.log(HELP); return 0; }
  if (args.selfTest) return selfTest(fileURLToPath(import.meta.url));
  if (args.positional.length !== 1) { console.error(HELP); return 1; }
  if (args.manifest) {
    try { console.log(JSON.stringify(manifestPage(args.positional[0], args))); return 0; } catch (e) { console.error(e.message); return 1; }
  }
  return render(args);
}

if (isMainModule(import.meta.url)) process.exitCode = main();
