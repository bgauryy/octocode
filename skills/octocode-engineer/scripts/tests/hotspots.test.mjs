import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { measure, parseLog } from '../hotspots.mjs';

const SCRIPT = fileURLToPath(new URL('../hotspots.mjs', import.meta.url));
const ENV = {
  ...process.env, GIT_AUTHOR_NAME: 'Test', GIT_AUTHOR_EMAIL: 'test@example.com', GIT_COMMITTER_NAME: 'Test',
  GIT_COMMITTER_EMAIL: 'test@example.com', GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1',
};

function repo(t) {
  const dir = mkdtempSync(join(tmpdir(), 'hotspots-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const git = (...args) => {
    const run = spawnSync('git', args, { cwd: dir, env: ENV, encoding: 'utf8' });
    assert.equal(run.status, 0, run.stderr);
    return run.stdout;
  };
  git('init', '-q');
  const commit = (files, date = '2026-01-01T00:00:00Z') => {
    for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
    git('add', '-A');
    spawnSync('git', ['commit', '-q', '-m', 'change'], { cwd: dir, env: { ...ENV, GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date } });
  };
  const hotspots = (...args) => {
    const run = spawnSync(process.execPath, [SCRIPT, ...args], { cwd: dir, encoding: 'utf8' });
    return { status: run.status, output: JSON.parse(run.stdout) };
  };
  return { dir, commit, hotspots };
}

const nested = (depth, unit = '  ') => Array.from({ length: depth + 1 }, (_, level) => `${unit.repeat(level)}line ${level}`).join('\n');

test('measure counts indentation levels in the file\'s own step, skipping blank lines', () => {
  const two = measure(`${nested(3)}\n\n`);
  const four = measure(nested(3, '    '));
  assert.equal(two.indentUnit, 2);
  assert.equal(four.indentUnit, 4);
  assert.equal(two.indent, four.indent, 'the same nesting scores the same in any indentation width');
  assert.equal(two.lines, 4);
  assert.equal(measure('\ta\n\t\tb\n').indent, 3, 'each tab is one level');
});

test('parseLog counts commits and churn per path and marks binary entries', () => {
  const log = '\x01aaa\0\n3\t1\tsrc/a.js\0-\t-\timg.png\0\x01bbb\0\n2\t0\tsrc/a.js\0';
  const { commits, files } = parseLog(log);
  assert.equal(commits, 2);
  assert.deepEqual(files.get('src/a.js'), { commits: 2, added: 5, deleted: 1, binary: false });
  assert.equal(files.get('img.png').binary, true);
});

test('files rank by commits × complexity, and the output states the formula and every input', (t) => {
  const { commit, hotspots } = repo(t);
  commit({ 'hot.js': nested(4), 'cold.js': nested(5), 'flat.txt': 'a\nb\n' });
  commit({ 'hot.js': `${nested(4)}\n// edit 1` });
  commit({ 'hot.js': `${nested(4)}\n// edit 2` });
  const { status, output } = hotspots();
  assert.equal(status, 0);
  assert.match(output.formula.score, /commits × complexity/);
  for (const file of output.files) assert.equal(file.score, file.commits * file.complexity);
  const scores = output.files.map((file) => file.score);
  assert.deepEqual(scores, [...scores].sort((a, b) => b - a), 'sorted by score');
  assert.equal(output.files[0].path, 'hot.js');
  assert.equal(output.summary.filesRanked, output.files.length);
  assert.equal(output.summary.commits, 3);
});

test('--complexity lines, --top, --since and pathspecs narrow or change the score', (t) => {
  const { commit, hotspots } = repo(t);
  commit({ 'old.js': nested(3) }, '2020-01-01T00:00:00Z');
  commit({ 'new.js': nested(3), 'other.js': nested(2) }, '2026-03-01T00:00:00Z');
  const lines = hotspots('--complexity', 'lines').output;
  for (const file of lines.files) assert.equal(file.complexity, file.lines);
  assert.equal(hotspots('--top', '1').output.files.length, 1);
  assert.equal(hotspots('--top', '1').output.summary.filesRanked, 3, 'the summary counts every file');
  assert.deepEqual(hotspots('--since', '2025-01-01').output.files.map((file) => file.path).sort(), ['new.js', 'other.js']);
  assert.deepEqual(hotspots('new.js').output.files.map((file) => file.path), ['new.js']);
});

test('deleted and binary files are skipped and counted', (t) => {
  const { dir, commit, hotspots } = repo(t);
  commit({ 'gone.js': nested(2), 'blob.bin': Buffer.from([0, 1, 2, 0]) });
  spawnSync('git', ['rm', '-q', 'gone.js'], { cwd: dir, env: ENV });
  commit({ 'kept.js': nested(1) });
  const { output } = hotspots();
  assert.deepEqual(output.files.map((file) => file.path), ['kept.js']);
  assert.equal(output.summary.skipped.binary, 1);
  assert.equal(output.summary.skipped.notInWorkingTree, 1);
});

test('bad options and a folder outside git are invalid input (exit 2)', (t) => {
  const { hotspots } = repo(t);
  for (const args of [['--since', 'yesterday'], ['--complexity', 'cyclomatic'], ['--top', '0'], ['--bogus']]) {
    const { status, output } = hotspots(...args);
    assert.equal(status, 2, args.join(' '));
    assert.equal(output.ok, false);
  }
  const outside = mkdtempSync(join(tmpdir(), 'hotspots-plain-'));
  t.after(() => rmSync(outside, { recursive: true, force: true }));
  const run = spawnSync(process.execPath, [SCRIPT], { cwd: outside, encoding: 'utf8', env: { ...ENV, GIT_CEILING_DIRECTORIES: tmpdir() } });
  assert.equal(run.status, 2);
  assert.equal(JSON.parse(run.stdout).error.code, 'notAGitRepository');
});
