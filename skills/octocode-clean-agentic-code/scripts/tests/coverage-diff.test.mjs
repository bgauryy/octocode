import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compareCoverage, parseCoverage } from '../coverage-diff.mjs';

const script = fileURLToPath(new URL('../coverage-diff.mjs', import.meta.url));
const fixtures = fileURLToPath(new URL('fixtures/coverage-diff/', import.meta.url));

function run(args, cwd = fixtures) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8', env: { PATH: process.env.PATH } });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

const counts = (covered, total) => ({ covered, total });
const file = (lines, branches = counts(0, 0)) => ({ lines, branches });

test('lists files with more uncovered lines or branches, lines up checkouts by root and exits 1', () => {
  const { status, json } = run(['before.lcov', 'after.lcov', '--before-root', '/work/base', '--after-root', '/work/head']);
  assert.equal(status, 1);
  assert.equal(json.ok, false);
  assert.equal(json.branchesCompared, true);
  assert.deepEqual(json.lost, [
    { file: 'src/added.js', lines: { before: null, after: counts(1, 2), lost: 1 }, branches: { before: null, after: counts(0, 0), lost: 0 } },
    { file: 'src/kept.js', lines: { before: counts(2, 3), after: counts(1, 3), lost: 1 }, branches: { before: counts(2, 2), after: counts(1, 2), lost: 1 } },
  ]);
  assert.deepEqual(json.removed, [{ file: 'src/deleted.js', uncoveredLines: 2, uncoveredBranches: 0 }]);
  assert.deepEqual(json.counts, { lostFiles: 2, linesLost: 2, branchesLost: 1, removedFiles: 1 });
});

test('deleting covered code, merging records and LF/LH-only records are not losses', () => {
  const { json } = run(['before.lcov', 'after.lcov', '--before-root', '/work/base', '--after-root', '/work/head']);
  const files = json.lost.map(entry => entry.file);
  assert.ok(!files.includes('src/shrunk.js'), 'shrunk.js removed covered lines and gained coverage on line 2');
  assert.ok(!files.includes('src/summary-only.js'));
});

test('without matching roots the same files do not line up', () => {
  const { json } = run(['before.lcov', 'after.lcov']);
  assert.equal(json.removed.length, 4);
});

test('reads istanbul json-summary, istanbul coverage-final and coverage.py json', () => {
  const summary = { total: { lines: { total: 4, covered: 3 } }, '/repo/a.js': { lines: { total: 4, covered: 3, pct: 75 }, branches: { total: 2, covered: 1 } } };
  assert.deepEqual([...parseCoverage(JSON.stringify(summary), '/repo', 's').files], [['a.js', file(counts(3, 4), counts(1, 2))]]);
  const final = { '/repo/b.js': { path: '/repo/b.js', statementMap: { 0: { start: { line: 1 } }, 1: { start: { line: 1 } }, 2: { start: { line: 2 } } }, s: { 0: 0, 1: 4, 2: 0 }, b: { 0: [1, 0] } } };
  assert.deepEqual([...parseCoverage(JSON.stringify(final), '/repo', 'f').files], [['b.js', file(counts(1, 2), counts(1, 2))]]);
  const py = { meta: { version: '7' }, files: { 'pkg/mod.py': { summary: { num_statements: 10, covered_lines: 7, num_branches: 4, covered_branches: 2 } } } };
  assert.deepEqual([...parseCoverage(JSON.stringify(py), '/repo', 'p').files], [['pkg/mod.py', file(counts(7, 10), counts(2, 4))]]);
});

test('branches are compared only when both reports carry branch data', () => {
  const before = new Map([['a.js', file(counts(2, 2), counts(0, 0))]]);
  const after = new Map([['a.js', file(counts(2, 2), counts(0, 4))]]);
  const result = compareCoverage(before, after);
  assert.equal(result.branchesCompared, false);
  assert.deepEqual(result.lost, []);
});

test('an unknown report format is invalid input', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coverage-diff-'));
  try {
    writeFileSync(join(dir, 'report.txt'), 'All files | 100 |\n');
    const { status, json } = run(['report.txt', 'report.txt'], dir);
    assert.equal(status, 2);
    assert.equal(json.error.code, 'unknown-format');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
