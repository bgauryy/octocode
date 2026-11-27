import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(new URL('../test-at-base.mjs', import.meta.url));
const GIT_ENV = {
  ...process.env,
  GIT_CONFIG_NOSYSTEM: '1',
  GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null',
  GIT_AUTHOR_NAME: 'fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
  GIT_COMMITTER_NAME: 'fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid',
};

function git(cwd, ...args) {
  const result = spawnSync('git', args, { cwd, env: GIT_ENV, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

// Base: add() is wrong. Head: add() is fixed and the PR adds a test that checks it, plus one that passes either way.
function fixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'test-at-base-fixture-')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repo = join(root, 'repo');
  mkdirSync(join(repo, 'pkg', 'test'), { recursive: true });
  git(repo, 'init', '-q');
  writeFileSync(join(repo, 'pkg', 'add.mjs'), 'export const add = (a, b) => a - b;\nexport const one = () => 1;\n');
  git(repo, 'add', '.');
  git(repo, 'commit', '-qm', 'base');
  const base = git(repo, 'rev-parse', 'HEAD');
  writeFileSync(join(repo, 'pkg', 'add.mjs'), 'export const add = (a, b) => a + b;\nexport const one = () => 1;\n');
  writeFileSync(join(repo, 'pkg', 'test', 'add.test.mjs'), "import { add } from '../add.mjs';\nif (add(2, 1) !== 3) process.exit(1);\n");
  writeFileSync(join(repo, 'pkg', 'test', 'one.test.mjs'), "import { one } from '../add.mjs';\nif (one() !== 1) process.exit(1);\n");
  git(repo, 'add', '.');
  git(repo, 'commit', '-qm', 'head');
  return { root, repo, base, head: git(repo, 'rev-parse', 'HEAD') };
}

function runScript(cwd, args) {
  const result = spawnSync(process.execPath, [SCRIPT, ...args], { cwd, env: GIT_ENV, encoding: 'utf8' });
  return { status: result.status, json: JSON.parse(result.stdout.trim().split('\n').at(-1)), stderr: result.stderr };
}

const worktrees = repo => git(repo, 'worktree', 'list', '--porcelain').split('\n').filter(line => line.startsWith('worktree '));

test('a test that fails at base and passes at head covers the change; the worktree is removed', t => {
  const { root, repo, base, head } = fixture(t);
  const logs = join(root, 'logs');
  const { status, json } = runScript(join(repo, 'pkg'), ['--base', base, '--head', head, '--test', 'test/add.test.mjs',
    '--cmd', `"${process.execPath}" test/add.test.mjs`, '--logs', logs]);
  assert.equal(status, 0, JSON.stringify(json));
  assert.equal(json.covers, true);
  assert.equal(json.base.rev, base);
  assert.notEqual(json.base.exit, 0);
  assert.equal(json.head.rev, head);
  assert.equal(json.head.exit, 0);
  assert.equal(json.head.dirty, false);
  assert.deepEqual(json.tests, [join('pkg', 'test', 'add.test.mjs')]);
  assert.ok(existsSync(json.base.log) && existsSync(json.head.log));
  assert.equal(worktrees(repo).length, 1);
  assert.equal(git(repo, 'status', '--porcelain'), '');
});

test('a test that passes at base does not cover the change', t => {
  const { root, repo, base } = fixture(t);
  const { status, json } = runScript(repo, ['--base', base, '--test', 'pkg/test/one.test.mjs',
    '--cmd', `"${process.execPath}" pkg/test/one.test.mjs`, '--logs', join(root, 'logs')]);
  assert.equal(status, 0, JSON.stringify(json));
  assert.equal(json.covers, false);
  assert.equal(json.base.exit, 0);
  assert.equal(json.head.exit, 0);
});

test('logs kept inside the checkout do not mark head dirty; an uncommitted edit does', t => {
  const { repo, base } = fixture(t);
  const args = ['--base', base, '--test', 'pkg/test/one.test.mjs', '--cmd', 'true',
    '--logs', join(repo, '.octocode', 'tmp', 'octocode-pr-review', 'test-at-base', 'named')];
  assert.equal(runScript(repo, args).json.head.dirty, false);
  assert.equal(runScript(repo, args).json.head.dirty, false);
  writeFileSync(join(repo, 'pkg', 'add.mjs'), 'export const add = () => 0;\n');
  assert.equal(runScript(repo, args).json.head.dirty, true);
});

test('without --logs, each run keeps its logs in its own folder under the workspace scratch, never the OS temp dir', t => {
  const { repo, base } = fixture(t);
  const args = ['--base', base, '--test', 'pkg/test/one.test.mjs', '--cmd', 'true'];
  const runs = [runScript(repo, args).json, runScript(repo, args).json];
  const scratch = join(repo, '.octocode', 'tmp', 'octocode-pr-review', 'test-at-base');
  for (const run of runs) {
    assert.equal(dirname(run.logs), scratch, run.logs);
    assert.ok(existsSync(run.head.log) && existsSync(run.base.log));
    assert.equal(run.head.dirty, false, 'its own logs do not mark head dirty');
  }
  assert.notEqual(runs[0].logs, runs[1].logs);
});

test('a failed setup is a failed run, and the worktree is still removed', t => {
  const { root, repo, base } = fixture(t);
  const { status, json } = runScript(repo, ['--base', base, '--test', 'pkg/test/add.test.mjs', '--setup', 'exit 3',
    '--cmd', 'true', '--logs', join(root, 'logs')]);
  assert.equal(status, 1);
  assert.equal(json.ok, false);
  assert.equal(json.error.code, 'setupFailed');
  assert.equal(json.setup.exit, 3);
  assert.equal(worktrees(repo).length, 1);
});

test('invalid input exits 2 with a stable code and creates no worktree', t => {
  const { repo, base } = fixture(t);
  const cases = [
    [['--test', 'pkg/test/add.test.mjs', '--cmd', 'true'], 'invalidArguments'],
    [['--base', base, '--cmd', 'true'], 'invalidArguments'],
    [['--base', base, '--test', 'pkg/test/add.test.mjs'], 'invalidArguments'],
    [['--base', base, '--test', 'pkg/test/add.test.mjs', '--cmd', 'true', '--bogus'], 'invalidArguments'],
    [['--base', 'no-such-rev', '--test', 'pkg/test/add.test.mjs', '--cmd', 'true'], 'unknownRevision'],
    [['--base', base, '--head', base, '--test', 'pkg/test/add.test.mjs', '--cmd', 'true'], 'headMismatch'],
    [['--base', base, '--test', 'pkg/test/missing.test.mjs', '--cmd', 'true'], 'testMissing'],
    [['--base', base, '--test', '../outside.mjs', '--cmd', 'true'], 'testOutsideRepository'],
  ];
  for (const [args, code] of cases) {
    const { status, json } = runScript(repo, args);
    assert.equal(status, 2, `${args.join(' ')} → ${JSON.stringify(json)}`);
    assert.deepEqual([json.ok, json.error.code], [false, code]);
  }
  assert.equal(worktrees(repo).length, 1);
});

test('an interrupt stops the command and removes the worktree', async t => {
  const { root, repo, base } = fixture(t);
  const marker = join(root, 'started');
  const wait = `"${process.execPath}" -e "require('fs').writeFileSync(process.argv[1], String(process.pid)); setInterval(() => {}, 1000)" "${marker}"`;
  const child = spawn(process.execPath, [SCRIPT, '--base', base, '--test', 'pkg/test/add.test.mjs', '--cmd', wait,
    '--logs', join(root, 'logs')], { cwd: repo, env: GIT_ENV });
  let stdout = '';
  child.stdout.on('data', chunk => { stdout += chunk; });
  const closed = new Promise(done => child.on('close', (code, signal) => done({ code, signal })));
  // The marker appears once the base command runs inside the worktree.
  await new Promise((done, fail) => {
    const poll = setInterval(() => {
      if (existsSync(marker) && readFileSync(marker, 'utf8')) { clearInterval(poll); done(); }
    }, 10);
    child.on('close', () => { clearInterval(poll); fail(new Error(`exited before the command started: ${stdout}`)); });
  });
  assert.equal(worktrees(repo).length, 2);
  child.kill('SIGTERM');
  const { code } = await closed;
  assert.notEqual(code, 0);
  assert.equal(JSON.parse(stdout.trim()).error.code, 'interrupted');
  assert.equal(worktrees(repo).length, 1);
  // The command's process group was signalled too: its process exits instead of outliving the script.
  const pid = Number(readFileSync(marker, 'utf8'));
  const alive = () => { try { process.kill(pid, 0); return true; } catch { return false; } };
  for (let tries = 0; tries < 500 && alive(); tries++) await new Promise(done => setTimeout(done, 10));
  assert.equal(alive(), false);
});

test('--help documents inputs, outputs and exit codes', () => {
  const result = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(result.status, 0);
  for (const word of ['--base', '--test', '--cmd', '--setup', 'covers', 'Exit codes']) assert.match(result.stdout, new RegExp(word));
});
