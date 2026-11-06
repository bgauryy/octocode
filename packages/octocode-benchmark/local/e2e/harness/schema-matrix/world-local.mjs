// The local fixture, written fresh per worker: the same known files under a git
// work tree (where .gitignore decides) and a plain directory (where the default
// prune list decides). Every file's role is stated here, not inferred.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const FILES = {
  'src/alpha.ts': [
    '// alpha comment Alpha',
    'export interface AlphaShape { size: number }',
    'export function alphaCall(name: string): string {',
    '  return ALPHA_TOKEN + alphaHelper(name);',
    '}',
    'export function alphaHelper(value: string): string {',
    '  return `a.pha aXpha ${value}`;',
    '}',
    'const ALPHA_TOKEN = "alphabet";',
    'const unrelated = "beta";',
    'probe(1);',
    '',
  ],
  'src/beta.py': ['def alpha_call(name):', '    return alpha_helper(name)', '', 'def beta_only():', '    return 1', ''],
  'src/gamma.txt': ['gamma', 'delta', 'plain words', ''],
  'src/empty.txt': [''],
  'src/lib.rs': ['pub fn alpha_lib() -> u32 { 1 }', '', '#[cfg(test)]', 'mod tests {', '    #[test]', '    fn alpha_case() {}', '}', ''],
  'src/.dot.ts': ['export const alphaDot = 1;', 'probe(2);', ''],
  'tests/alpha.test.ts': ['import { alphaCall } from "../src/alpha";', 'alphaCall("test");', 'probe(3);', ''],
  'src/util_test.go': ['package util', 'func alphaGo() {}', ''],
  '.hidden/dot.ts': ['export const alphaHidden = 1;', 'probe(4);', ''],
  'ignored-dir/ig.ts': ['export const alphaIgnored = 1;', 'probe(5);', ''],
  'build/gen.ts': ['export const alphaBuilt = 1;', 'probe(6);', ''],
  'node_modules/dep/index.ts': ['export const alphaDep = 1;', 'probe(7);', ''],
  'run.sh': ['#!/bin/sh', 'echo alpha', ''],
};
/** The declarations an outline owes per file, stated by hand. */
export const SYMBOLS = {
  'src/alpha.ts': ['AlphaShape', 'size', 'alphaCall', 'alphaHelper', 'ALPHA_TOKEN', 'unrelated'],
  'src/beta.py': ['alpha_call', 'beta_only'],
  'src/lib.rs': ['alpha_lib', 'tests', 'alpha_case'],
  'src/.dot.ts': ['alphaDot'],
  'tests/alpha.test.ts': [],
  'src/util_test.go': ['alphaGo'],
  '.hidden/dot.ts': ['alphaHidden'],
  'ignored-dir/ig.ts': ['alphaIgnored'],
  'build/gen.ts': ['alphaBuilt'],
  'node_modules/dep/index.ts': ['alphaDep'],
};
/** One small source per grammar, each spelling a `greet` call or label in its own syntax. */
export const LANG = {
  typescript: ['a.ts', ['greet(1);', '']],
  tsx: ['a.tsx', ['const view = <div>{greet(1)}</div>;', '']],
  javascript: ['a.js', ['greet(1);', '']],
  python: ['a.py', ['greet(1)', '']],
  go: ['a.go', ['package a', 'func run() { greet(1) }', '']],
  rust: ['a.rs', ['fn run() { greet(1); }', '']],
  java: ['A.java', ['class A { void run() { greet(1); } }', '']],
  c: ['a.c', ['void run(void) { greet(1); }', '']],
  asm: ['a.s', ['greet:', '  nop', '']],
  scala: ['a.scala', ['object A { def run() = greet(1) }', '']],
  cpp: ['a.cpp', ['void run() { greet(1); }', '']],
  cuda: ['a.cu', ['void run() { greet(1); }', '']],
  csharp: ['a.cs', ['class A { void Run() { greet(1); } }', '']],
};
/** Fixed snippets for rule fields whose effect is about position. */
export const SHAPES = {
  'scope.ts': ['probe(', '  1', ');', 'probe(2);', ''],
  'nth.ts': ['list(a1, b2, c3);', ''],
  'nest.ts': ['function outer() {', '  if (ready) {', '    return 1;', '  }', '}', ''],
};
const GITIGNORE = 'ignored-dir/\nbuild/\n';
/** Lines of a file that test-code rules claim on their own: the item under `#[cfg(test)]`, its attribute line included, 1-based. */
const TEST_LINES = { 'src/lib.rs': [3, 4, 5, 6, 7] };
const TEST_FILES = new Set(['tests/alpha.test.ts', 'src/util_test.go']);
const DEFAULT_PRUNED = new Set(['build', 'node_modules']);
const IGNORED_DIRS = new Set(['ignored-dir', 'build']);

export const textOf = file => (file === '.gitignore' ? GITIGNORE : FILES[file].join('\n'));
export const fixtureFiles = Object.keys(FILES);
export const fixtureTexts = () => Object.fromEntries(fixtureFiles.map(file => [file, textOf(file)]));
const segmentsOf = file => file.split('/');
const isHidden = file => segmentsOf(file).some(segment => segment.startsWith('.'));
const isIgnoredFile = file => IGNORED_DIRS.has(segmentsOf(file)[0]);
const isPruned = file => DEFAULT_PRUNED.has(segmentsOf(file)[0]);
export const isTestFile = file => TEST_FILES.has(file);
export const testLines = file => new Set(TEST_LINES[file] ?? []);

/** Files the walk may reach, from the fields that scope it. `plain` is not a git tree. */
export function visibleFiles(root, query) {
  const files = [...fixtureFiles, ...(root === 'git' ? ['.gitignore'] : [])];
  return files.filter(file => (query.includeHidden === true || !isHidden(file))
    && (root !== 'git' || query.includeIgnored === true || !isIgnoredFile(file))
    && (query.defaultExcludes === false || !isPruned(file) || (root === 'git' && !isIgnoredFile(file))));
}

/** Whether a test-code mode keeps a file: `src/lib.rs` is mixed, so it stays in every mode. */
export function testModeKeeps(mode, file) {
  if (mode === 'exclude') return !isTestFile(file);
  if (mode === 'only') return isTestFile(file) || testLines(file).size > 0;
  return true;
}

/** A line's fate under a test-code mode. */
export function testModeKeepsLine(mode, file, line) {
  if (!testModeKeeps(mode, file)) return false;
  if (mode === 'exclude') return !testLines(file).has(line);
  if (mode === 'only') return isTestFile(file) || testLines(file).has(line);
  return true;
}

function writeTree(root, withGit) {
  for (const [file, lines] of Object.entries(FILES)) {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, lines.join('\n'));
  }
  fs.chmodSync(path.join(root, 'run.sh'), 0o755);
  if (withGit) {
    fs.writeFileSync(path.join(root, '.gitignore'), GITIGNORE);
    const git = args => spawnSync('git', ['-c', 'user.name=m', '-c', 'user.email=m@example.invalid', ...args], { cwd: root, encoding: 'utf8' });
    git(['init', '-q', '.']);
    git(['add', '-A']);
    git(['commit', '-qm', 'fixture']);
  }
}

/** Distinct, known times per file, opposite orders for modified and accessed. */
function stampTimes(root) {
  const base = Date.UTC(2024, 0, 1) / 1000;
  const files = [...fixtureFiles];
  files.forEach((file, index) => {
    const modified = base + index * 3600;
    const accessed = base + (files.length - index) * 3600;
    fs.utimesSync(path.join(root, file), accessed, modified);
  });
}

/** Directory holding one file per access mode, for the fields only a listing checks. */
function writePermissionTree(root) {
  fs.mkdirSync(root, { recursive: true });
  const base = Date.UTC(2024, 0, 1) / 1000;
  for (const [index, [name, mode]] of Object.entries({ 'open.txt': 0o644, 'locked.txt': 0o000, 'frozen.txt': 0o444, 'tool.sh': 0o755 }).entries()) {
    const file = path.join(root, name);
    fs.writeFileSync(file, 'permission fixture\n');
    fs.utimesSync(file, base + index * 3600, base + index * 3600);
    fs.chmodSync(file, mode);
  }
}

export function buildLocalWorld(base) {
  const world = path.join(base, 'matrix-world');
  writeTree(path.join(world, 'plain'), false);
  writeTree(path.join(world, 'git'), true);
  stampTimes(path.join(world, 'plain'));
  stampTimes(path.join(world, 'git'));
  writePermissionTree(path.join(world, 'perm'));
  const put = (root, file, lines) => { fs.mkdirSync(path.join(world, root), { recursive: true }); fs.writeFileSync(path.join(world, root, file), lines.join('\n')); };
  for (const [file, lines] of Object.values(LANG)) put('lang', file, lines);
  for (const [file, lines] of Object.entries(SHAPES)) put('shapes', file, lines);
  return { dir: world, plain: path.join(world, 'plain'), git: path.join(world, 'git'), perm: path.join(world, 'perm'), lang: path.join(world, 'lang'), shapes: path.join(world, 'shapes') };
}
