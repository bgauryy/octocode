#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, mkdirSync, writeFileSync, chmodSync, rmSync, rmdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './scripts/lib/entrypoint.mjs';
import { buildSync } from 'esbuild';

const here = dirname(fileURLToPath(import.meta.url));
const bundles = [
  {
    trigger: 'scripts/render-report.mjs',
    files: [
      'scripts/render-report.mjs',
      'scripts/lib/entrypoint.mjs',
      // The renderer's modules: every file in scripts/lib/report/ ships with it.
      ...readdirSync(join(here, 'scripts/lib/report')).filter(name => name.endsWith('.mjs')).toSorted().map(name => `scripts/lib/report/${name}`),
    ],
  },
  {
    trigger: 'assets/template.html',
    files: [{ source: 'assets/architecture.html', target: 'assets/template.html' }],
  },
  {
    trigger: 'scripts/lib/net-guard.mjs',
    files: ['scripts/lib/net-guard.mjs'],
  },
  {
    trigger: 'scripts/lib/fail.mjs',
    files: ['scripts/lib/fail.mjs'],
  },
  {
    trigger: 'scripts/lib/entrypoint.mjs',
    files: ['scripts/lib/entrypoint.mjs'],
  },
  {
    trigger: 'scripts/lib/config-env.mjs',
    files: [{ source: 'scripts/config-env.entry.mjs', target: 'scripts/lib/config-env.mjs', bundle: true }],
  },
];

const bundled = new Set(bundles.flatMap(bundle => bundle.files.map(file => typeof file === 'string' ? file : file.target)));
for (const entry of readdirSync(join(here, 'references'))) {
  const target = `references/${entry}`;
  if (entry.endsWith('.md') && !bundled.has(target)) bundles.push({ trigger: target, files: [target] });
}

function referencedPaths(text, file, root) {
  return [...text.matchAll(/\]\(\s*<?([^)\s>#]+)(?:#[^)\s>]*)?>?(?:\s+["'][^)]*)?\)/g)]
    .filter(([, target]) => !/^[a-z]+:/i.test(target))
    .map(([, target]) => relative(root, resolve(dirname(file), target)).replaceAll('\\', '/'));
}

const fileEntry = (file) => typeof file === 'string' ? { source: file, target: file, bundle: false } : { bundle: false, ...file };

/** The skill collections at the repository root: the one list every skill gate walks. */
export const COLLECTIONS = ['skills', 'skills-dev'];

/** Skill folders (those with a SKILL.md) in every collection under root. */
function skillDirs(root) {
  const dirs = [];
  for (const collection of COLLECTIONS) {
    const folder = join(root, collection);
    if (!existsSync(folder)) continue;
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      const dir = join(folder, entry.name);
      if (entry.isDirectory() && existsSync(join(dir, 'SKILL.md'))) dirs.push(dir);
    }
  }
  return dirs;
}

const ROUTE_EXTENSIONS = /\.(md|mjs|js|cjs)$/;

/** Every file under dir as a skill-relative path (dot-folders and node_modules skipped). */
function skillFiles(dir, base = dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    if (entry.name.startsWith('.') || entry.name === 'node_modules') return [];
    const file = join(dir, entry.name);
    if (entry.isDirectory()) return skillFiles(file, base);
    return entry.isFile() ? [relative(base, file).replaceAll('\\', '/')] : [];
  });
}

/** Relative import/require specifiers in script text, resolved to skill-relative paths. */
function importedPaths(text, file, root) {
  return [...text.matchAll(/(?:from\s*|import\s*\(\s*|require\s*\(\s*)['"](\.{1,2}\/[^'"]+)['"]/g)]
    .map(([, target]) => relative(root, resolve(dirname(file), target)).replaceAll('\\', '/'));
}

/** True when text names the path as a whole path, not as the tail of a longer one. */
function names(text, path) {
  let from = text.indexOf(path);
  while (from !== -1) {
    if (from === 0 || !/[\w./-]/.test(text[from - 1])) return true;
    from = text.indexOf(path, from + 1);
  }
  return false;
}

/** Known skill-relative paths a document reaches: links, imports, and whole paths it names. */
function routesFrom(text, path, root, known) {
  const file = join(root, path);
  const reached = new Set([...referencedPaths(text, file, root), ...importedPaths(text, file, root)]);
  const fileDir = dirname(path);
  for (const candidate of known) {
    if (reached.has(candidate) || candidate === path) continue;
    const local = fileDir === '.' ? candidate : relative(fileDir, candidate).replaceAll('\\', '/');
    if (names(text, candidate) || (!local.startsWith('.') && names(text, local))) reached.add(candidate);
  }
  return reached;
}

/** Every installed path a shared bundle may occupy in a skill. */
export function sharedTargets() {
  return new Set(bundles.flatMap(bundle => bundle.files.map(file => fileEntry(file).target)));
}

/**
 * The route graph of one skill: what an agent can load by starting at SKILL.md.
 * `files` are the authored files reached; `bundles` the shared bundles whose entry a reached document
 * names or links; `links` every shared path a reached document names or links, which must be installed.
 */
export function routeGraph(dir) {
  const managed = sharedTargets();
  const authored = new Set(skillFiles(dir).filter(file => !managed.has(file)));
  const known = new Set([...authored, ...managed]);
  const files = new Set(['SKILL.md']);
  const selected = new Set();
  const links = new Set();
  const queue = [{ path: 'SKILL.md', text: readFileSync(join(dir, 'SKILL.md'), 'utf8') }];
  while (queue.length) {
    const { path, text } = queue.shift();
    for (const next of routesFrom(text, path, dir, known)) {
      if (authored.has(next) && !files.has(next)) {
        files.add(next);
        if (ROUTE_EXTENSIONS.test(next)) queue.push({ path: next, text: readFileSync(join(dir, next), 'utf8') });
      }
      if (!managed.has(next)) continue;
      links.add(next);
      for (const bundle of bundles) {
        if (bundle.trigger !== next || selected.has(bundle)) continue;
        selected.add(bundle);
        // Shared pages are traversed from canonical content, so a fresh install gets the whole chain.
        for (const { source, target } of bundle.files.map(fileEntry).filter(f => f.source.endsWith('.md'))) {
          queue.push({ path: target, text: readFileSync(join(here, source), 'utf8') });
        }
      }
    }
  }
  return { files, bundles: selected, links };
}

/** The bundles a skill uses: those its route graph reaches from SKILL.md. */
function selectBundles(dir) {
  return routeGraph(dir).bundles;
}

/** target → canonical source for the selected bundles. */
function bundleOutputs(selected) {
  const outputs = new Map();
  for (const bundle of selected) {
    for (const { source, target, bundle: usesBundle } of bundle.files.map(fileEntry)) {
      if (outputs.has(target) && (outputs.get(target).source !== source || outputs.get(target).bundle !== usesBundle)) throw new Error(`Conflicting shared sources for ${target}`);
      outputs.set(target, { source, bundle: usesBundle });
    }
  }
  return outputs;
}

/** Copies one shared file unless it is current; returns whether it was stale. */
function syncCopy(destination, content, target, check) {
  if (existsSync(destination) && readFileSync(destination).equals(content)) return false;
  if (check) return true;
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, content);
  if (target.endsWith('.mjs')) chmodSync(destination, 0o755);
  return true;
}

/** Removes a copy no route reaches, and the folders that leaves empty; returns whether it was stale. */
function removeCopy(dir, target, content, check) {
  const destination = join(dir, target);
  // Only byte-identical copies are ours to delete; anything else at a shared path is authored.
  if (!existsSync(destination) || !readFileSync(destination).equals(content)) return false;
  if (check) return true;
  rmSync(destination);
  for (let folder = dirname(destination); folder !== dir && readdirSync(folder).length === 0; folder = dirname(folder)) rmdirSync(folder);
  return true;
}

/** Installed paths and canonical bytes of the given bundles, including bundled transitive guide dependencies. */
function canonicalAssets(selected, bundledResources) {
  const outputs = new Map();
  for (const [target, { source, bundle }] of bundleOutputs(selected)) {
    let content;
    if (bundle) {
      if (!bundledResources.has(source)) {
        // This is generated distribution code; keep the bundle compact while the authored entry stays readable.
        const result = buildSync({ absWorkingDir: here, entryPoints: [source], bundle: true, platform: 'node', format: 'esm', target: 'node24', minify: true, write: false });
        bundledResources.set(source, Buffer.from(result.outputFiles[0].contents));
      }
      content = bundledResources.get(source);
    } else {
      content = readFileSync(join(here, source));
    }
    outputs.set(target, content);
  }
  return outputs;
}

/** Installed paths and canonical bytes for the bundles a skill's routes reach. */
export function sharedAssets(dir, bundledResources = new Map()) {
  return canonicalAssets(selectBundles(dir), bundledResources);
}

export function build({ root = resolve(here, '../..'), check = false } = {}) {
  const stale = [];
  let copies = 0;
  const dirs = skillDirs(root);
  const bundledResources = new Map();
  const everything = canonicalAssets(new Set(bundles), bundledResources);
  for (const dir of dirs) {
    const wanted = sharedAssets(dir, bundledResources);
    for (const [target, content] of wanted) {
      const destination = join(dir, target);
      copies++;
      if (syncCopy(destination, content, target, check)) stale.push(relative(root, destination));
    }
    for (const [target, content] of everything) {
      if (!wanted.has(target) && removeCopy(dir, target, content, check)) stale.push(relative(root, join(dir, target)));
    }
  }
  return { skills: dirs.length, copies, stale };
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== '--check')) {
    console.error('Usage: node skills/common/build.mjs [--check]');
    process.exit(args.includes('--help') ? 0 : 1);
  }
  const check = args.includes('--check');
  const result = build({ check });
  for (const file of result.stale) console.log(`${check ? 'STALE' : 'UPDATED'} ${file}`);
  console.log(`${result.skills} skills inspected; ${result.copies} shared files; ${result.stale.length} ${check ? 'stale' : 'updated'}.`);
  process.exitCode = check && result.stale.length ? 1 : 0;
}
