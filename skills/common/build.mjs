#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, mkdirSync, writeFileSync, chmodSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './scripts/lib/entrypoint.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const bundles = [
  {
    trigger: 'scripts/render-report.mjs',
    files: [
      'scripts/render-report.mjs',
      'scripts/lib/entrypoint.mjs',
      // The renderer's modules: every file in scripts/lib/report/ ships with it.
      ...readdirSync(join(here, 'scripts/lib/report')).filter(name => name.endsWith('.mjs')).toSorted().map(name => `scripts/lib/report/${name}`),
      'references/report-output.md',
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
];

const bundled = new Set(bundles.flatMap(bundle => bundle.files.map(file => typeof file === 'string' ? file : file.target)));
for (const entry of readdirSync(join(here, 'references'))) {
  const target = `references/${entry}`;
  if (entry.endsWith('.md') && !bundled.has(target)) bundles.push({ trigger: target, files: [target] });
}

function markdownFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    if (entry.name.startsWith('.') || entry.name === 'node_modules') return [];
    const file = join(dir, entry.name);
    if (entry.isDirectory()) return markdownFiles(file);
    return entry.isFile() && entry.name.endsWith('.md') ? [file] : [];
  });
}

function referencedPaths(text, file, root) {
  return [...text.matchAll(/\]\(\s*<?([^)\s>#]+)(?:#[^)\s>]*)?>?(?:\s+["'][^)]*)?\)/g)]
    .filter(([, target]) => !/^[a-z]+:/i.test(target))
    .map(([, target]) => relative(root, resolve(dirname(file), target)).replaceAll('\\', '/'));
}

const fileEntry = (file) => typeof file === 'string' ? { source: file, target: file } : file;

/** Skill folders (those with a SKILL.md) in every collection under root. */
function skillDirs(root) {
  const dirs = [];
  for (const collection of ['skills', 'skills-beta', 'skills-dev']) {
    const folder = join(root, collection);
    if (!existsSync(folder)) continue;
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      const dir = join(folder, entry.name);
      if (entry.isDirectory() && existsSync(join(dir, 'SKILL.md'))) dirs.push(dir);
    }
  }
  return dirs;
}

/** The bundles a skill uses: named or linked by its docs, already installed, or linked from another selected bundle. */
function selectBundles(dir) {
  const docs = markdownFiles(dir).map(file => ({ file, text: readFileSync(file, 'utf8') }));
  const references = new Set(docs.flatMap(({ file, text }) => referencedPaths(text, file, dir)));
  const text = docs.map(doc => doc.text).join('\n');
  const wanted = (bundle) => references.has(bundle.trigger) || text.includes(bundle.trigger) || existsSync(join(dir, bundle.trigger));
  const selected = new Set();
  // Resolve shared-reference dependencies from canonical content, including on a fresh install.
  let added = true;
  while (added) {
    added = false;
    for (const bundle of bundles) {
      if (selected.has(bundle) || !wanted(bundle)) continue;
      selected.add(bundle);
      added = true;
      for (const { source, target } of bundle.files.map(fileEntry).filter(f => f.source.endsWith('.md'))) {
        for (const link of referencedPaths(readFileSync(join(here, source), 'utf8'), join(dir, target), dir)) references.add(link);
      }
    }
  }
  return selected;
}

/** target → canonical source for the selected bundles. */
function bundleOutputs(selected) {
  const outputs = new Map();
  for (const bundle of selected) {
    for (const { source, target } of bundle.files.map(fileEntry)) {
      if (outputs.has(target) && outputs.get(target) !== source) throw new Error(`Conflicting shared sources for ${target}`);
      outputs.set(target, source);
    }
  }
  return outputs;
}

/** Copies one shared file unless it is current; returns whether it was stale. */
function syncCopy(destination, source, target, check) {
  const content = readFileSync(join(here, source));
  if (existsSync(destination) && readFileSync(destination).equals(content)) return false;
  if (check) return true;
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, content);
  if (target.endsWith('.mjs')) chmodSync(destination, 0o755);
  return true;
}

export function build({ root = resolve(here, '../..'), check = false } = {}) {
  const stale = [];
  let copies = 0;
  const dirs = skillDirs(root);
  for (const dir of dirs) {
    for (const [target, source] of bundleOutputs(selectBundles(dir))) {
      const destination = join(dir, target);
      copies++;
      if (syncCopy(destination, source, target, check)) stale.push(relative(root, destination));
    }
  }
  return { skills: dirs.length, copies, stale };
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== '--check')) throw new Error('Usage: node skills/common/build.mjs [--check]');
  const check = args.includes('--check');
  const result = build({ check });
  for (const file of result.stale) console.log(`${check ? 'STALE' : 'UPDATED'} ${file}`);
  console.log(`${result.skills} skills inspected; ${result.copies} shared files; ${result.stale.length} ${check ? 'stale' : 'updated'}.`);
  process.exitCode = check && result.stale.length ? 1 : 0;
}
