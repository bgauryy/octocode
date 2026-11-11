#!/usr/bin/env node
// Removes what agent runs leave behind in this checkout. Dry by default: prints what it would
// remove and how much it frees; `--apply` removes it. Never touches the shared native target/
// (see `dev clean:cache`), source files, reports, or a git worktree with uncommitted changes.
//
// What counts as a leftover:
//   - private cargo target dirs under .octocode/tmp/targets that no live process names
//   - pinned test binaries (.octocode/**/bin*/octocode) older than --days (default 2)
//   - `octocode serve` daemons started from a binary under .octocode/ or one that no longer exists
//   - graph snapshots whose scope directory no longer exists
//   - clean, detached git worktrees under .octocode/tmp/worktrees (dirty ones are only listed)
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from './lib/workspaces.mjs';

const args = process.argv.slice(2);
const apply = args.includes('--apply');
const daysAt = args.indexOf('--days');
const days = daysAt >= 0 ? Number(args[daysAt + 1]) : 2;
if (!Number.isFinite(days) || days < 0) {
  console.error('clean-leftovers: --days takes a number of days');
  process.exit(2);
}
const OCTO = path.join(ROOT, '.octocode');
const cutoff = Date.now() - days * 86_400_000;

const run = (cmd, argv, cwd = ROOT) => {
  try {
    return execFileSync(cmd, argv, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {
    return '';
  }
};
const processes = run('ps', ['-axo', 'pid=,command=']).split('\n').filter(Boolean).map(line => {
  const text = line.trim();
  const at = text.indexOf(' ');
  return { pid: Number(text.slice(0, at)), command: text.slice(at + 1) };
});
const named = dir => processes.some(p => p.command.includes(dir));
const sizeOf = target => {
  const out = run('du', ['-sk', target]);
  return Number(out.split('\t')[0] || 0) * 1024;
};
const dirs = parent => {
  try {
    return fs.readdirSync(parent, { withFileTypes: true }).filter(e => e.isDirectory()).map(e => path.join(parent, e.name));
  } catch {
    return [];
  }
};

const found = [];
const kept = [];
const add = (kind, target, act, bytes = sizeOf(target)) => found.push({ kind, target, act, bytes });

// Private cargo targets.
for (const dir of dirs(path.join(OCTO, 'tmp', 'targets'))) {
  if (named(dir)) kept.push(`${path.relative(ROOT, dir)}: a live process names it`);
  else add('private target', dir, () => fs.rmSync(dir, { recursive: true, force: true }));
}

// Pinned test binaries, by age.
const walk = (dir, depth) => {
  if (depth > 6) return;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (!entry.isDirectory()) continue;
    if (entry.name === 'node_modules' || entry.name === 'graph' || full === path.join(OCTO, 'tmp')) continue;
    const binary = path.join(full, 'octocode');
    if (/^bin\d*$/.test(entry.name) && fs.existsSync(binary)) {
      const stat = fs.statSync(binary);
      if (stat.mtimeMs >= cutoff) kept.push(`${path.relative(ROOT, binary)}: newer than ${days} day(s)`);
      else if (named(binary)) kept.push(`${path.relative(ROOT, binary)}: a live process runs it`);
      else add('pinned binary', binary, () => fs.rmSync(binary, { force: true }), stat.size);
      continue;
    }
    walk(full, depth + 1);
  }
};
if (fs.existsSync(OCTO)) walk(OCTO, 0);

// Daemons of binaries that are gone or pinned under .octocode.
for (const p of processes) {
  const match = /^(\S+\/octocode) serve --socket /.exec(p.command);
  if (!match) continue;
  const binary = match[1];
  const gone = !fs.existsSync(binary);
  if (!gone && !binary.startsWith(OCTO + path.sep)) continue;
  add(gone ? 'daemon of a removed binary' : 'daemon of a pinned binary', `pid ${p.pid} ${path.relative(ROOT, binary)}`, () => {
    try {
      process.kill(p.pid, 'SIGTERM');
    } catch {
      /* already gone */
    }
  }, 0);
}

// Graph snapshots whose scope is gone.
for (const dir of dirs(path.join(OCTO, 'graph'))) {
  let root;
  try {
    root = JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf8')).root;
  } catch {
    continue;
  }
  if (typeof root !== 'string') continue;
  const scope = path.isAbsolute(root) ? root : path.join(ROOT, root);
  if (!fs.existsSync(scope)) add('graph snapshot of a removed scope', dir, () => fs.rmSync(dir, { recursive: true, force: true }));
}

// Worktrees: clean ones go, dirty ones are named.
const worktrees = run('git', ['worktree', 'list', '--porcelain']).split('\n').filter(l => l.startsWith('worktree ')).map(l => l.slice(9));
for (const tree of worktrees) {
  if (!tree.startsWith(path.join(OCTO, 'tmp', 'worktrees') + path.sep)) continue;
  const dirty = run('git', ['status', '--porcelain'], tree).split('\n').filter(Boolean).length;
  if (dirty > 0) kept.push(`${path.relative(ROOT, tree)}: worktree with ${dirty} uncommitted change(s) — decide by hand`);
  else if (named(tree)) kept.push(`${path.relative(ROOT, tree)}: a live process names it`);
  else add('clean worktree', tree, () => run('git', ['worktree', 'remove', tree]));
}

const mb = bytes => `${(bytes / 1_048_576).toFixed(0)} MB`;
let total = 0;
for (const item of found) {
  total += item.bytes;
  console.log(`${apply ? 'removed' : 'would remove'}  ${item.kind}: ${path.isAbsolute(item.target) ? path.relative(ROOT, item.target) : item.target}  (${mb(item.bytes)})`);
  if (apply) item.act();
}
for (const line of kept) console.log(`kept  ${line}`);
console.log(`clean-leftovers: ${found.length} item(s), ${mb(total)}${apply ? ' removed' : ' — rerun with --apply to remove'}`);
