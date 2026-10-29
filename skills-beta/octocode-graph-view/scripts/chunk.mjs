#!/usr/bin/env node
// Large repo: one graph snapshot has a file budget, so plan chunks, build the biggest, and link them in an overview.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { flags } from './lib.mjs';

const USAGE = `usage: chunk.mjs <root> [--out <dir>] [--chunk-files 1500] [--build 3] [--only a,b] [--area <path>] [--max-files 30]
  Splits <root> into directories of at most --chunk-files files (graph ingest refuses a section over its
  snapshot budget), writes <out>/plan.json, builds data.json for the --build largest chunks (or --only slugs)
  with build.mjs, renders each built chunk (report.html, brief.txt) and writes <out>/overview/data.json: chunks as nodes, workspace-package imports as edges.`;

const t0 = Date.now();
const argv = process.argv.slice(2);
const f = flags(argv, { out: '.octocode/octocode-graph-view/tmp', 'chunk-files': 1500, build: 3, only: '', area: '', 'max-files': 30 });
const root = argv[0] && !argv[0].startsWith('-') ? resolve(argv[0]) : null;
if (!root) {
  console.error(USAGE);
  process.exit(2);
}
const out = resolve(f.out);
const limit = Number(f['chunk-files']);
const here = dirname(fileURLToPath(import.meta.url));

// File list: tracked files when git knows the root, else a walk that skips build output.
function listFiles() {
  try {
    return execFileSync('git', ['-C', root, 'ls-files', '--cached', '--others', '--exclude-standard'], { encoding: 'utf8', maxBuffer: 256 << 20 }).split('\n').filter(Boolean);
  } catch {
    const acc = [];
    (function walk(dir) {
      for (const d of readdirSync(dir, { withFileTypes: true })) {
        if (d.name.startsWith('.') || ['node_modules', 'target', 'dist', 'build'].includes(d.name)) continue;
        if (d.isDirectory()) walk(join(dir, d.name));
        else acc.push(relative(root, join(dir, d.name)));
      }
    })(root);
    return acc;
  }
}
const area = f.area.replace(/^\.?\/|\/$/g, '').replace(/^\.$/, '');
const files = listFiles().filter((p) => existsSync(join(root, p)) && (!area || p === area || p.startsWith(`${area}/`)));
const tree = new Map();
for (const p of files) {
  const parts = p.split('/');
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join('/');
    tree.set(dir, (tree.get(dir) ?? 0) + 1);
  }
}
const children = (dir) => [...tree.keys()].filter((k) => dirname(k) === (dir || '.') && k !== dir);
const chunks = [];
(function split(dir) {
  const count = dir && dir !== '.' ? tree.get(dir) ?? 0 : files.length;
  const kids = children(dir);
  if (count <= limit || !kids.length) {
    if (count >= 10) chunks.push({ path: dir || '.', files: count });
    return;
  }
  // Files that sit directly in a split directory belong to no child: give them a chunk of their own.
  const loose = count - kids.reduce((a, k) => a + (tree.get(k) ?? 0), 0);
  if (loose >= 10) chunks.push({ path: dir || '.', files: loose, loose: true, exclude: kids.map((k) => basename(k)) });
  kids.forEach(split);
})(area);
chunks.sort((a, b) => b.files - a.files);
const slug = (p) => p.replace(/[^A-Za-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'root';
const used = new Set();
chunks.forEach((c) => { let id = slug(c.path) + (c.loose ? '-loose' : ''), n = 2; while (used.has(id)) id = `${slug(c.path)}-${n++}`; used.add(id); c.id = id; });

// Package names let imports of workspace packages become edges between chunks.
function manifestName(dir) {
  const abs = join(root, dir);
  try {
    const pj = join(abs, 'package.json');
    if (existsSync(pj)) return JSON.parse(readFileSync(pj, 'utf8')).name;
    const ct = join(abs, 'Cargo.toml');
    if (existsSync(ct)) return readFileSync(ct, 'utf8').match(/^name\s*=\s*"([^"]+)"/m)?.[1];
  } catch {}
}
chunks.forEach((c) => { c.package = manifestName(c.path); });

// Declared dependencies (package.json, Cargo.toml) link chunks without ingesting them.
function declaredDeps(dir) {
  const abs = join(root, dir);
  const names = new Set();
  try {
    const pj = join(abs, 'package.json');
    if (existsSync(pj)) {
      const m = JSON.parse(readFileSync(pj, 'utf8'));
      for (const k of ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies']) Object.keys(m[k] ?? {}).forEach((n) => names.add(n));
    }
    const ct = join(abs, 'Cargo.toml');
    if (existsSync(ct)) {
      let inDeps = false;
      for (const line of readFileSync(ct, 'utf8').split('\n')) {
        if (/^\[/.test(line)) inDeps = /^\[(workspace\.)?(dev-|build-)?dependencies\]/.test(line);
        else if (inDeps) { const k = line.match(/^([A-Za-z0-9_-]+)\s*[.=]/)?.[1]; if (k) names.add(k); }
      }
    }
  } catch {}
  return names;
}

mkdirSync(join(out, 'chunks'), { recursive: true });
const wanted = f.only ? new Set(f.only.split(',')) : new Set(chunks.slice(0, Number(f.build)).map((c) => c.id));
for (const c of chunks.filter((x) => wanted.has(x.id))) {
  const dir = join(out, 'chunks', c.id);
  const r = spawnSync(process.execPath, [join(here, 'build.mjs'), join(root, c.path), '--out', dir, '--max-files', String(f['max-files']), ...(c.exclude ? ['--exclude', c.exclude.join(',')] : [])], { encoding: 'utf8' });
  if (r.status === 0) c.built = dir;
  else {
    const lines = String(r.stderr || r.stdout || r.error?.message || `exit ${r.status ?? r.signal}`).trim().split('\n');
    const at = lines.findIndex((l) => /^\w*Error: /.test(l));
    c.error = (at < 0 ? lines.slice(-2) : lines.slice(at).filter((l) => !/^\s+at |^Node\.js /.test(l))).join(' ').slice(0, 200);
  }
}

for (const c of chunks) if (!c.built && existsSync(join(out, 'chunks', c.id, 'data.json'))) c.built = join(out, 'chunks', c.id);

for (const c of chunks.filter((x) => x.built)) spawnSync(process.execPath, [join(here, 'render.mjs'), '--data', join(c.built, 'data.json')], { stdio: 'ignore' });

// Overview: chunks as nodes; an edge when a built chunk imports another chunk's package.
const byPackage = new Map(chunks.filter((c) => c.package).map((c) => [`pkg:${c.package}`, c]));
const link = new Map();
const addLink = (a, b, via, weight = 1) => {
  const k = `${a.id}>${b.id}`;
  const cur = link.get(k) ?? { source: a.id, target: b.id, line: 0, via: new Set() };
  cur.via.add(via);
  cur.line += weight;
  link.set(k, cur);
};
for (const c of chunks) for (const n of declaredDeps(c.path)) { const t = byPackage.get(`pkg:${n}`); if (t && t !== c) addLink(c, t, 'declared'); }
for (const c of chunks.filter((x) => x.built)) {
  const d = JSON.parse(readFileSync(join(c.built, 'data.json'), 'utf8'));
  c.findings = d.insights.findings;
  c.filesInCycles = d.insights.health?.filesInCycles;
  for (const x of d.externals ?? []) {
    const t = byPackage.get(x.id);
    if (t && t !== c) addLink(c, t, 'observed', x.count);
  }
}
const edges = [...link.values()].map((e) => ({ ...e, via: [...e.via].join('+') }));
const fan = (id, k) => edges.filter((e) => e[k] === id).length;
const baseCount = new Map();
chunks.forEach((c) => baseCount.set(basename(c.path), (baseCount.get(basename(c.path)) ?? 0) + 1));
const labelOf = (c) => (c.path === '.' ? basename(root) : baseCount.get(basename(c.path)) > 1 ? c.path.split('/').slice(-2).join('/') : basename(c.path));
const nodes = chunks.map((c) => ({ id: c.id, label: labelOf(c), group: c.path.split('/')[0], fanIn: fan(c.id, 'target'), fanOut: fan(c.id, 'source'), ...(c.built ? { href: `../chunks/${c.id}/report.html` } : {}) }));
const total = files.length;
const overview = {
  section: root,
  frozen: false,
  mode: 'overview',
  area: area || null,
  uncovered: total - chunks.reduce((a, c) => a + c.files, 0),
  snapshot: { id: 'overview', root, counts: { nodes: chunks.length, edges: edges.length, nodesByKind: { file: total } } },
  files: nodes,
  imports: edges,
  symbols: [],
  calls: [],
  fnFiles: [],
  legends: { files: [`Chunks: ${chunks.length}`, `Links: ${edges.length}`], functions: ['Files: 0', 'Functions: 0', 'Calls: 0'] },
  insights: { hubs: [], health: {}, findings: {}, issues: [], cycles: { total: 0, top: [] }, groups: [], mutualGroups: [] },
  hubFiles: [],
  tokens: { sourceBytes: 0, sourceFiles: total },
  chunks: chunks.map(({ id, path, files, loose, package: pkg, built, error, findings, filesInCycles }) => ({ id, path, files, loose, package: pkg, built: !!built, error, findings, filesInCycles })),
};
mkdirSync(join(out, 'overview'), { recursive: true });
writeFileSync(join(out, 'overview', 'data.json'), JSON.stringify(overview));
writeFileSync(join(out, 'plan.json'), JSON.stringify(overview.chunks, null, 1));
console.log(JSON.stringify({ ms: Date.now() - t0, out, files: total, chunks: chunks.length, built: chunks.filter((c) => c.built).map((c) => c.id), failed: chunks.filter((c) => c.error).map((c) => [c.id, c.error]), links: edges.length }));
