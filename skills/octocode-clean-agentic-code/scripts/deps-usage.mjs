#!/usr/bin/env node
// Compares a package.json with the imports of the package's own JS/TS sources: declared but never imported,
// imported but undeclared, production dependencies imported only by tests, and names declared twice. Built-ins only.
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { builtinModules } from 'node:module';
import { basename, dirname, extname, join, relative, resolve } from 'node:path';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const USAGE = `Usage: node deps-usage.mjs <package-dir>

Reads <package-dir>/package.json and every JS/TS source of that package (.js .mjs .cjs .jsx .ts .tsx .mts .cts .vue
.svelte .astro). Inside a git work tree only tracked and untracked-but-not-ignored files are read; elsewhere the walk
skips dot folders (not dot files such as .eslintrc.json), node_modules, dist, build, out and coverage. Folders with their own package.json are other
packages and are skipped. Imports counted: import/export from, side-effect import, import(), require(),
require.resolve(), import.meta.resolve(), jest/vi mock and requireActual/importActual, and /// <reference types>.
Comments are ignored; relative paths, # subpath imports, URL schemes and Node builtins are not dependencies.
@types/x counts as used when x is imported or declared (@types/node when a builtin is imported).
A declared name that no source imports but that package.json scripts or a config file at the package root
(*.config.*, .*rc*, tsconfig*.json) names, directly or by an installed bin, is listed as referencedOnly.
JS/TS manifests only: for other ecosystems use the octocode-graph issues detector undeclared-dependency.

Output (stdout, JSON): {"ok","package","manifest","filesScanned","declared","unimported","undeclared",
"testOnlyInDependencies","inMultipleSections","referencedOnly"}
  unimported               [{"name","section","binsChecked"}]: declared, never imported or referenced
                           (binsChecked false: the package is not installed, so its bin names were not compared)
  undeclared               [{"name","sites":[{"file","line"}],"declaredBy"?}]: imported, absent from this manifest;
                           declaredBy names an ancestor package.json that declares it (a hoisted phantom)
  testOnlyInDependencies   [{"name","sites"}]: in dependencies, imported only from test files
  inMultipleSections       [{"name","sections"}]: in dependencies and devDependencies, or peer and dependencies
  referencedOnly           [{"name","section","in"}]: not imported, named by scripts or config (keep unless proven)
Reads files and git state only; changes nothing.
Exit 0: none of unimported, undeclared, testOnlyInDependencies, inMultipleSections. 1: at least one.
2: invalid input (no package.json, unreadable JSON): {"ok":false,"error":{"code","message"}}.`;

const SOURCE = new Set(['js', 'mjs', 'cjs', 'jsx', 'ts', 'tsx', 'mts', 'cts', 'vue', 'svelte', 'astro']);
const SECTIONS = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies'];
const WALK_SKIP = new Set(['node_modules', 'dist', 'build', 'out', 'coverage']);
const BUILTINS = new Set(builtinModules);
const TEST_DIRS = new Set(['test', 'tests', '__tests__', 'spec', 'specs', 'e2e', '__mocks__', 'fixtures', '__fixtures__']);
const TEST_FILE = /\.(?:test|spec)\.[^.]+$|^vitest\.|^jest\.|^playwright\./;
const CONFIG_FILE = /^(?:\.[\w-]+rc(?:\.[\w]+)?|[\w.-]+\.config\.[\w]+|tsconfig[\w.-]*\.json)$/;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** Index of the quote closing the string literal opened at start (a plain string also ends at a newline). */
function stringEnd(text, start) {
  const quote = text[start];
  for (let j = start + 1; j < text.length; j++) {
    if (text[j] === '\\') j++;
    else if (text[j] === quote || (quote !== '`' && text[j] === '\n')) return j;
  }
  return text.length;
}

/** Source text with comments blanked to spaces (same length and newlines, so offsets and lines hold); strings stay. */
export function stripComments(text) {
  let out = '';
  let i = 0;
  while (i < text.length) {
    const char = text[i];
    const next = text[i + 1];
    if (char === '/' && next === '/') {
      const end = text.indexOf('\n', i);
      const stop = end === -1 ? text.length : end;
      out += ' '.repeat(stop - i);
      i = stop;
    } else if (char === '/' && next === '*') {
      const end = text.indexOf('*/', i + 2);
      const stop = end === -1 ? text.length : end + 2;
      out += text.slice(i, stop).replaceAll(/[^\n]/g, ' ');
      i = stop;
    } else if (char === '"' || char === "'" || char === '`') {
      const end = stringEnd(text, i);
      out += text.slice(i, end + 1);
      i = end + 1;
    } else {
      out += char;
      i++;
    }
  }
  return out;
}

const IMPORT_PATTERNS = [
  /(?:^|[^.\w$])(?:import|export)\s+(?:type\s+)?(?:[\w$*{}\s,]+?\s+from\s+)?['"]([^'"\n]+)['"]/g,
  /(?:^|[^.\w$])import\s*\(\s*['"]([^'"\n]+)['"]\s*[,)]/g,
  /\brequire(?:\.resolve)?\s*\(\s*['"]([^'"\n]+)['"]\s*\)/g,
  /\bimport\.meta\.resolve\s*\(\s*['"]([^'"\n]+)['"]\s*\)/g,
  /\b(?:jest|vi)\.(?:mock|doMock|unmock|requireActual|importActual)\s*(?:<[^>]*>)?\s*\(\s*['"]([^'"\n]+)['"]/g,
];

/** The package an import specifier names, or null for relative, subpath, URL-scheme and builtin specifiers. */
export function packageName(specifier) {
  if (/^[./#]/.test(specifier) || /^[a-z][a-z0-9+.-]*:/i.test(specifier) || /[\s${}]/.test(specifier)) return null;
  const parts = specifier.split('/');
  const name = specifier.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
  return BUILTINS.has(name) ? null : name;
}

/** The @types package that carries a package's types: @scope/pkg → @types/scope__pkg. */
const typesPackage = name => `@types/${name.replace(/^@([^/]+)\//, '$1__')}`;

const isBuiltin = specifier => specifier.startsWith('node:') || BUILTINS.has(specifier.split('/')[0]);

/** Imports in one source: [{ name, line }] for packages, and whether any builtin is imported. */
export function importsOf(text) {
  const imports = [];
  let builtin = false;
  const lineOf = offset => text.slice(0, offset).split('\n').length;
  for (const found of text.matchAll(/\/\/\/[ \t]*<reference[ \t]+types[ \t]*=[ \t]*['"]([^'"]+)['"]/g)) {
    // TypeScript resolves the name through @types/<name> or the package itself.
    const name = found[1] === 'node' ? 'node' : packageName(found[1]);
    if (name) imports.push({ name, line: lineOf(found.index), types: true });
  }
  const code = stripComments(text);
  for (const pattern of IMPORT_PATTERNS) {
    for (const found of code.matchAll(pattern)) {
      const specifier = found[1];
      if (isBuiltin(specifier) && !specifier.startsWith('@')) builtin = true;
      const name = packageName(specifier);
      if (name) imports.push({ name, line: lineOf(found.index + found[0].indexOf(specifier)) });
    }
  }
  return { imports, builtin };
}

export function classifyTest(rel) {
  const segments = rel.split('/');
  return segments.slice(0, -1).some(segment => TEST_DIRS.has(segment)) || TEST_FILE.test(segments.at(-1));
}

function git(cwd, args) {
  const run = spawnSync('git', ['-c', 'core.quotePath=false', ...args], { cwd, encoding: 'utf8', maxBuffer: 1 << 30 });
  return run.status === 0 ? run.stdout : null;
}

function walk(dir, root, out) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!entry.name.startsWith('.') && !WALK_SKIP.has(entry.name)) walk(full, root, out);
    } else if (entry.isFile()) {
      out.push(relative(root, full).replaceAll('\\', '/'));
    }
  }
}

/** Package-relative paths of every file in the package, nested packages excluded. */
function packageFiles(dir) {
  const top = git(dir, ['rev-parse', '--show-toplevel'])?.trim();
  let files = [];
  if (top) {
    const listed = git(dir, ['ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', '.']) ?? '';
    files = listed.split('\0').filter(Boolean).filter(file => !file.split('/').includes('node_modules'));
  } else {
    walk(dir, dir, files);
  }
  const nested = files.filter(file => basename(file) === 'package.json' && file !== 'package.json').map(file => `${dirname(file)}/`);
  return files.filter(file => !nested.some(prefix => file.startsWith(prefix)));
}

function readJson(path) {
  try { return JSON.parse(readFileSync(path, 'utf8')); } catch (error) { throw new InputError('invalid-json', `${path}: ${errorMessage(error)}`); }
}

/** Bin names of an installed dependency, looked up from the package folder upward; null when not installed. */
function binsOf(dir, name) {
  for (let folder = dir; ; folder = dirname(folder)) {
    const manifest = join(folder, 'node_modules', name, 'package.json');
    if (existsSync(manifest)) {
      try {
        const { bin } = JSON.parse(readFileSync(manifest, 'utf8'));
        if (typeof bin === 'string') return [name.split('/').pop()];
        return bin && typeof bin === 'object' ? Object.keys(bin) : [];
      } catch {
        return [];
      }
    }
    if (dirname(folder) === folder) return null;
  }
}

/** The nearest ancestor package.json above dir that declares name, as a path relative to dir. */
function ancestorDeclaring(dir, name) {
  for (let folder = dirname(dir); dirname(folder) !== folder; folder = dirname(folder)) {
    const manifest = join(folder, 'package.json');
    if (!existsSync(manifest)) continue;
    try {
      const json = JSON.parse(readFileSync(manifest, 'utf8'));
      if (SECTIONS.some(section => json[section]?.[name] !== undefined)) return relative(dir, manifest).replaceAll('\\', '/');
    } catch {
      // an unreadable ancestor manifest declares nothing we can cite
    }
  }
  return undefined;
}

const escape = text => text.replaceAll(/[.*+?^${}()|[\]\\]/g, '\\$&');
const names = (text, word) => new RegExp(`(?<![\\w@/.-])${escape(word)}(?![\\w-])`).test(text);

export function analyze(dir) {
  const manifestPath = join(dir, 'package.json');
  if (!existsSync(manifestPath)) throw new InputError('no-manifest', `${dir}: no package.json`);
  const manifest = readJson(manifestPath);
  const declared = new Map(); // name → sections
  for (const section of SECTIONS) for (const name of Object.keys(manifest[section] ?? {})) declared.set(name, [...(declared.get(name) ?? []), section]);
  const files = packageFiles(dir);
  const sites = new Map(); // name → [{ file, line, test }]
  let builtin = false;
  let scanned = 0;
  for (const file of files.filter(item => SOURCE.has(extname(item).slice(1).toLowerCase()))) {
    let text;
    try {
      if (!lstatSync(join(dir, file)).isFile()) continue;
      text = readFileSync(join(dir, file), 'utf8');
    } catch {
      continue; // listed by git but deleted from the work tree
    }
    scanned++;
    const found = importsOf(text);
    builtin ||= found.builtin;
    const test = classifyTest(file);
    for (const { name, line, types } of found.imports) sites.set(name, [...(sites.get(name) ?? []), { file, line, test, types: types === true }]);
  }
  const used = name => {
    if (sites.has(name)) return true;
    if (!name.startsWith('@types/')) return false;
    const target = name.slice('@types/'.length).replace(/^([^_]+)__/, '@$1/');
    return sites.has(target) || declared.has(target) || ((target === 'node' || BUILTINS.has(target)) && builtin);
  };
  const scripts = Object.values(manifest.scripts ?? {}).join('\n');
  const commandWords = new Set(scripts.split(/[\s;&|()]+/));
  const configs = files.filter(file => !file.includes('/') && CONFIG_FILE.test(file)).map(file => {
    try { return { file, text: readFileSync(join(dir, file), 'utf8') }; } catch { return { file, text: '' }; }
  });
  const unimported = [];
  const referencedOnly = [];
  for (const [name, sections] of declared) {
    if (used(name)) continue;
    const bins = binsOf(dir, name);
    const where = [];
    if (names(scripts, name) || (bins ?? []).some(bin => commandWords.has(bin))) where.push('package.json scripts');
    for (const { file, text } of configs) if (names(text, name)) where.push(file);
    for (const section of sections) {
      if (where.length) referencedOnly.push({ name, section, in: where });
      else unimported.push({ name, section, binsChecked: bins !== null });
    }
  }
  const strip = list => list.map(({ file, line }) => ({ file, line }));
  const undeclared = [];
  for (const [name, list] of [...sites].toSorted(([a], [b]) => a.localeCompare(b))) {
    if (declared.has(name) || name === manifest.name) continue;
    if (list.every(site => site.types) && declared.has(typesPackage(name))) continue;
    const declaredBy = ancestorDeclaring(dir, name);
    undeclared.push({ name, sites: strip(list), ...(declaredBy ? { declaredBy } : {}) });
  }
  const testOnlyInDependencies = [...declared]
    .filter(([name, sections]) => sections.includes('dependencies') && sites.has(name) && sites.get(name).every(site => site.test))
    .map(([name]) => ({ name, sites: strip(sites.get(name)) }));
  const inMultipleSections = [...declared]
    .filter(([, sections]) => (sections.includes('dependencies') && sections.includes('devDependencies')) || (sections.includes('dependencies') && sections.includes('peerDependencies')))
    .map(([name, sections]) => ({ name, sections }));
  const byName = (a, b) => a.name.localeCompare(b.name);
  return {
    package: manifest.name ?? null,
    manifest: manifestPath,
    filesScanned: scanned,
    declared: declared.size,
    unimported: unimported.toSorted(byName),
    undeclared,
    testOnlyInDependencies: testOnlyInDependencies.toSorted(byName),
    inMultipleSections: inMultipleSections.toSorted(byName),
    referencedOnly: referencedOnly.toSorted(byName),
  };
}

export function main(argv = process.argv.slice(2)) {
  if (argv.includes('--help') || argv.includes('-h')) { console.log(USAGE); process.exit(0); }
  const unknown = argv.find(arg => arg.startsWith('-'));
  if (unknown) fail('invalid-args', `unknown option ${unknown}; see --help`, 2);
  if (argv.length !== 1) fail('invalid-args', 'expected one <package-dir>; see --help', 2);
  const dir = resolve(argv[0]);
  if (!existsSync(dir) || !statSync(dir).isDirectory()) fail('missing-path', `${argv[0]}: not a directory`, 2);
  let result;
  try {
    result = analyze(dir);
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('scan-failed', errorMessage(error), 2);
  }
  const findings = result.unimported.length + result.undeclared.length + result.testOnlyInDependencies.length + result.inMultipleSections.length;
  console.log(JSON.stringify({ ok: findings === 0, ...result }, null, 2));
  process.exit(findings ? 1 : 0);
}

if (isMainModule(import.meta.url)) main();
