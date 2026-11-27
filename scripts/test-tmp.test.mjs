// Contract of scripts/test-tmp.mjs: one private temp root per test run under
// <repo>/.octocode/tmp/t/, handed to the command as TMPDIR/TMP/TEMP, removed when
// the run ends (pass, fail or signal), refused up front when a socket under it
// would not fit sockaddr_un.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  ENV_NAMES,
  SOCKET_PATH_MAX,
  rootFor,
  socketProblem,
  socketSuffixes,
} from './test-tmp.mjs';

const SCRIPT = fileURLToPath(new URL('./test-tmp.mjs', import.meta.url));
const REPO = path.dirname(path.dirname(SCRIPT));
const posix = process.platform !== 'win32';

/** A working directory named like a package folder, so the root's name is known. */
function packageDir(t, name = 'octocode-probe') {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'test-tmp-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const dir = path.join(base, name);
  fs.mkdirSync(dir);
  return dir;
}

/** `node -e <source>` as the wrapped command. */
const nodeEval = source => [process.execPath, '-e', source];

/** Reports the child's temp variables and tmpdir(), and leaves a file in it. */
const REPORT = `
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
fs.writeFileSync(path.join(os.tmpdir(), 'left-behind.txt'), 'x');
const env = Object.fromEntries(${JSON.stringify(ENV_NAMES)}.map(name => [name, process.env[name]]));
console.log(JSON.stringify({ env, tmpdir: os.tmpdir(), isDir: fs.statSync(os.tmpdir()).isDirectory() }));
`;

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

test('the command gets a fresh root as TMPDIR, TMP and TEMP, removed once it passes', t => {
  const cwd = packageDir(t);
  const run = spawnSync(process.execPath, [SCRIPT, ...nodeEval(REPORT)], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  const seen = JSON.parse(run.stdout);
  const root = rootFor(REPO, cwd, run.pid);
  assert.equal(path.dirname(root), path.join(REPO, '.octocode', 'tmp', 't'));
  assert.equal(path.basename(root), `probe-${run.pid}`);
  for (const name of ENV_NAMES) assert.equal(seen.env[name], root, name);
  assert.equal(seen.tmpdir, root);
  assert.equal(seen.isDir, true);
  assert.equal(fs.existsSync(root), false, 'the root is removed');
});

test('a script written to the root runs as in the OS temp dir: no repo package scope or git repository above it', t => {
  const cwd = packageDir(t);
  // CommonJS without an extension, as a fake `cargo` on PATH is; the repo's package.json says "type": "module".
  const probe = `
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { spawnSync } = require('node:child_process');
const script = path.join(os.tmpdir(), 'fake-tool');
fs.writeFileSync(script, "const { tmpdir } = require('node:os'); console.log('cjs');");
const cjs = spawnSync(process.execPath, [script], { encoding: 'utf8' });
const git = spawnSync('git', ['rev-parse', '--show-toplevel'], { cwd: os.tmpdir(), encoding: 'utf8' });
console.log(JSON.stringify({ cjs: cjs.stdout.trim() || cjs.stderr, git: git.error ? null : git.status }));
`;
  const run = spawnSync(process.execPath, [SCRIPT, ...nodeEval(probe)], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  const seen = JSON.parse(run.stdout);
  assert.equal(seen.cjs, 'cjs');
  // null: no git installed, nothing to check.
  if (seen.git !== null)
    assert.notEqual(seen.git, 0, 'git finds no repository from the root');
});

test('a failing command keeps its exit code and its root is still removed', t => {
  const cwd = packageDir(t);
  const run = spawnSync(
    process.execPath,
    [SCRIPT, ...nodeEval(`${REPORT}; process.exit(3)`)],
    { cwd, encoding: 'utf8' }
  );
  assert.equal(run.status, 3, run.stderr);
  assert.equal(fs.existsSync(JSON.parse(run.stdout).tmpdir), false);
});

test('a command that cannot start fails the run and leaves no root', t => {
  const cwd = packageDir(t);
  const run = spawnSync(
    process.execPath,
    [SCRIPT, path.join(cwd, 'no-such-command')],
    {
      cwd,
      encoding: 'utf8',
    }
  );
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /no-such-command/);
  assert.equal(fs.existsSync(rootFor(REPO, cwd, run.pid)), false);
});

test('no command is a usage error', () => {
  const run = spawnSync(process.execPath, [SCRIPT], { encoding: 'utf8' });
  assert.equal(run.status, 2);
  assert.match(run.stderr, /Usage/);
});

for (const signal of ['SIGTERM', 'SIGINT']) {
  test(
    `${signal} reaches the command; the root goes and the run dies of ${signal}`,
    {
      skip: !posix,
    },
    async t => {
      const cwd = packageDir(t);
      const wrapper = spawn(
        process.execPath,
        [
          SCRIPT,
          ...nodeEval(
            `console.log(JSON.stringify({ pid: process.pid, tmpdir: require('node:os').tmpdir() })); setInterval(() => {}, 1000);`
          ),
        ],
        { cwd, stdio: ['ignore', 'pipe', 'inherit'] }
      );
      const started = await new Promise(resolve =>
        wrapper.stdout.once('data', chunk => resolve(JSON.parse(String(chunk))))
      );
      assert.equal(fs.existsSync(started.tmpdir), true);
      const exited = new Promise(resolve =>
        wrapper.once('exit', (code, sig) => resolve({ code, sig }))
      );
      wrapper.kill(signal);
      assert.deepEqual(await exited, { code: null, sig: signal });
      assert.equal(alive(started.pid), false, 'the command is gone');
      assert.equal(fs.existsSync(started.tmpdir), false, 'the root is removed');
    }
  );
}

test(
  'processes the run left working in the root are stopped before it goes',
  {
    skip: !posix,
  },
  async t => {
    const cwd = packageDir(t);
    // One stray keeps its working directory in the root, one only a Unix socket there.
    const strays = `
const { spawn } = require('node:child_process'), os = require('node:os'), path = require('node:path');
const stay = 'setInterval(() => {}, 1000)';
const inCwd = spawn(process.execPath, ['-e', stay], { cwd: os.tmpdir(), detached: true, stdio: 'ignore' });
const sock = path.join(os.tmpdir(), 's.sock');
const onSocket = spawn(process.execPath, ['-e', "require('node:net').createServer().listen(process.argv[1], () => console.log('up'))", sock], { cwd: '/', detached: true, stdio: ['ignore', 'pipe', 'ignore'] });
onSocket.stdout.once('data', () => { console.log(JSON.stringify([inCwd.pid, onSocket.pid])); process.exit(0); });
`;
    const run = spawnSync(process.execPath, [SCRIPT, ...nodeEval(strays)], {
      cwd,
      encoding: 'utf8',
    });
    assert.equal(run.status, 0, run.stderr);
    const pids = JSON.parse(run.stdout);
    t.after(() =>
      pids.filter(alive).forEach(pid => process.kill(pid, 'SIGKILL'))
    );
    assert.deepEqual(pids.filter(alive), [], 'every stray is stopped');
    assert.match(run.stderr, /stopped 2 process/);
    assert.equal(fs.existsSync(rootFor(REPO, cwd, run.pid)), false);
  }
);

test('the root name keeps the package folder short: no octocode- prefix, a bounded length', () => {
  const short = rootFor('/r', '/r/packages/octocode-mcp', 7);
  assert.equal(short, path.join('/r', '.octocode', 'tmp', 't', 'mcp-7'));
  const long = path.basename(
    rootFor('/r', '/r/packages/octocode-agents-communication', 7)
  );
  assert.ok('agents-communication'.startsWith(long.slice(0, -'-7'.length)));
  assert.ok(long.length < 'agents-communication-7'.length);
});

test(
  'the socket check refuses one byte past sockaddr_un and accepts the longest root that fits',
  {
    skip: !posix,
  },
  () => {
    const uid = process.getuid();
    const longest = Math.max(
      ...socketSuffixes(uid).map(({ suffix }) => Buffer.byteLength(suffix))
    );
    // The path and its NUL terminator must fit SOCKET_PATH_MAX bytes.
    const fits = `/${'r'.repeat(SOCKET_PATH_MAX - 1 - longest - 1)}`;
    assert.equal(socketProblem(fits, uid), null);
    const problem = socketProblem(`${fits}r`, uid);
    assert.match(problem, new RegExp(`${SOCKET_PATH_MAX}`));
    assert.match(problem, /sockaddr_un/);
  }
);

test(
  'a checkout too deep for its sockets is refused before anything is created',
  {
    skip: !posix,
  },
  t => {
    const deep = path.join(
      packageDir(t, 'octocode-deep'),
      'd'.repeat(SOCKET_PATH_MAX)
    );
    fs.mkdirSync(path.join(deep, 'scripts'), { recursive: true });
    const copy = path.join(deep, 'scripts', 'test-tmp.mjs');
    fs.copyFileSync(SCRIPT, copy);
    const marker = path.join(deep, 'ran');
    const run = spawnSync(
      process.execPath,
      [
        copy,
        ...nodeEval(
          `require('node:fs').writeFileSync(${JSON.stringify(marker)}, '')`
        ),
      ],
      { cwd: path.dirname(deep), encoding: 'utf8' }
    );
    assert.equal(run.status, 2);
    assert.match(run.stderr, /sockaddr_un/);
    assert.match(run.stderr, new RegExp(`${SOCKET_PATH_MAX}`));
    assert.equal(fs.existsSync(marker), false, 'the command did not run');
    assert.equal(
      fs.existsSync(path.join(deep, '.octocode')),
      false,
      'no root was created'
    );
  }
);
