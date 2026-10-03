#!/usr/bin/env node

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HELP = `Render an RFC artifact set as one offline-friendly HTML page and open it.

Usage:
  node scripts/render-rfc.mjs <rfc-folder-or-file> [--out <file.html>] [--no-open] [--json]
  node scripts/render-rfc.mjs --self-test
  node scripts/render-rfc.mjs --help

Reads every *.md in the RFC folder (RFC.md, PLAN.md, IMPLEMENTATION.md, ... first), embeds them
in assets/rfc-viewer.html, and writes one self-contained page: navigation per file, overview cards,
section TOC, search across files, mermaid diagrams, light/dark theme, print to PDF.
Default output: <workspace>/.octocode/tmp/octocode-rfc-generator/view/<name>/index.html when the RFC is
under <workspace>/.octocode/, else <os-tmp>/octocode-rfc-view/<name>/index.html.
Opens the page with the OS browser unless --no-open, CI is set, or no display is available.`;

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ASSETS = path.join(HERE, '..', 'assets');
const ORDER = ['RFC.md', 'PLAN.md', 'IMPLEMENTATION.md', 'PREREQUISITES.md', 'KPI.md', 'RESOURCES.md', 'AUDIT.md'];
const SKIP_DIRS = new Set(['node_modules', '.git', 'view', 'dist', 'build']);
const MAX_DEPTH = 3;

function collect(root) {
  const out = [];
  const walk = (dir, depth) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name.startsWith('.')) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) { if (depth < MAX_DEPTH && !SKIP_DIRS.has(entry.name)) walk(full, depth + 1); }
      else if (/\.md$/i.test(entry.name)) out.push(path.relative(root, full).split(path.sep).join('/'));
    }
  };
  walk(root, 0);
  const rank = (p) => { const i = ORDER.indexOf(p); return i === -1 ? ORDER.length + (p.includes('/') ? 1 : 0) : i; };
  return out.sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

function defaultOut(root, name) {
  const parts = root.split(path.sep);
  const i = parts.lastIndexOf('.octocode');
  const base = i > 0 ? path.join(parts.slice(0, i + 1).join(path.sep) || path.sep, 'tmp', 'octocode-rfc-generator', 'view')
    : path.join(os.tmpdir(), 'octocode-rfc-view');
  return path.join(base, name.replace(/[^\w.-]+/g, '-'), 'index.html');
}

// JSON inside <script type="application/json">: escape every '<' so no file content can close the tag.
const safeJson = (value) => JSON.stringify(value).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');

export function render(target, { out } = {}) {
  const abs = path.resolve(target);
  if (!fs.existsSync(abs)) throw new Error(`not found: ${target}`);
  const isFile = fs.statSync(abs).isFile();
  const root = isFile ? path.dirname(abs) : abs;
  const paths = isFile ? [path.basename(abs)] : collect(root);
  if (!paths.length) throw new Error(`no .md files in ${target}`);
  const files = paths.map((p) => ({ path: p, content: fs.readFileSync(path.join(root, p), 'utf8') }));
  const name = path.basename(root);
  const data = { title: '', root, generated: new Date().toISOString(), files };
  const shell = fs.readFileSync(path.join(ASSETS, 'rfc-viewer.html'), 'utf8');
  const js = fs.readFileSync(path.join(ASSETS, 'rfc-viewer.js'), 'utf8').replace(/<\/script/gi, '<\\/script');
  if (!shell.includes('__RFC_DATA__') || !shell.includes('/*__RFC_VIEWER_JS__*/')) throw new Error('viewer template placeholders missing');
  const html = shell.replace('__RFC_DATA__', () => safeJson(data)).replace('/*__RFC_VIEWER_JS__*/', () => js);
  const file = path.resolve(out || defaultOut(root, name));
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, html);
  return { out: file, root, files: paths, bytes: Buffer.byteLength(html) };
}

function canOpen() {
  if (process.env.CI) return false;
  if (process.platform === 'linux') return Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  return true;
}

function openInBrowser(file) {
  const url = 'file://' + file.split(path.sep).join('/').replace(/^([A-Za-z]):/, '/$1:');
  const [cmd, args] = process.platform === 'darwin' ? ['open', [file]]
    : process.platform === 'win32' ? ['cmd', ['/c', 'start', '', file]] : ['xdg-open', [file]];
  try { spawn(cmd, args, { detached: true, stdio: 'ignore' }).unref(); return url; } catch { return null; }
}

function selfTest() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'rfc-view-test-'));
  const rfc = path.join(dir, 'ws', '.octocode', 'rfc', 'demo');
  fs.mkdirSync(path.join(rfc, 'notes'), { recursive: true });
  fs.writeFileSync(path.join(rfc, 'PLAN.md'), '# Plan\n\n- [x] one\n- [ ] two\n');
  fs.writeFileSync(path.join(rfc, 'RFC.md'), '# Demo RFC\n\nStatus: Draft\n\nInject </script><script>alert(1)</script> \u2028 test.\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\nSee [plan](PLAN.md#steps).\n');
  fs.writeFileSync(path.join(rfc, 'zeta.md'), '# Zeta\n');
  fs.writeFileSync(path.join(rfc, 'notes', 'a.md'), '# Note\n');
  const checks = [];
  const check = (name, ok) => checks.push([name, Boolean(ok)]);
  const r = render(rfc);
  const html = fs.readFileSync(r.out, 'utf8');
  check('default output under workspace .octocode/tmp', r.out === path.join(dir, 'ws', '.octocode', 'tmp', 'octocode-rfc-generator', 'view', 'demo', 'index.html'));
  check('canonical file order', JSON.stringify(r.files) === JSON.stringify(['RFC.md', 'PLAN.md', 'zeta.md', 'notes/a.md']));
  const m = html.match(/<script id="rfc-data" type="application\/json">([\s\S]*?)<\/script>/);
  check('data block present', m);
  const data = m && JSON.parse(m[1]);
  check('content round-trips exactly', data && data.files[0].content === fs.readFileSync(path.join(rfc, 'RFC.md'), 'utf8'));
  check('no raw </script> from content', m && !m[1].includes('</script'));
  check('viewer script inlined', html.includes("'use strict'") && !html.includes('/*__RFC_VIEWER_JS__*/'));
  check('placeholders consumed', !html.includes('__RFC_DATA__'));
  const single = render(path.join(rfc, 'PLAN.md'), { out: path.join(dir, 'single.html') });
  check('single file input', single.files.length === 1 && fs.existsSync(path.join(dir, 'single.html')));
  let threw = false;
  try { render(path.join(dir, 'missing')); } catch { threw = true; }
  check('missing target fails', threw);
  fs.rmSync(dir, { recursive: true, force: true });
  for (const [name, ok] of checks) console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`);
  const failed = checks.filter(([, ok]) => !ok).length;
  console.log(failed ? `self-test failed (${failed}/${checks.length})` : `self-test passed (${checks.length} cases)`);
  return failed ? 1 : 0;
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); return 0; }
  if (argv.includes('--self-test')) return selfTest();
  const outIdx = argv.indexOf('--out');
  const out = outIdx >= 0 ? argv[outIdx + 1] : undefined;
  const target = argv.find((a, i) => !a.startsWith('--') && !(outIdx >= 0 && i === outIdx + 1));
  if (!target) { console.error(HELP); return 2; }
  let result;
  try { result = render(target, { out }); } catch (e) { console.error(`render-rfc: ${e.message}`); return 1; }
  const opened = !argv.includes('--no-open') && canOpen() ? openInBrowser(result.out) : null;
  if (argv.includes('--json')) console.log(JSON.stringify({ ...result, opened: Boolean(opened) }, null, 2));
  else console.log(`RFC viewer: ${result.out} (${result.files.length} files${opened ? ', opened in browser' : ''})`);
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
