import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  DEFAULT_PORT,
  liveSessions,
  noChromeMessage,
  probeFailure,
  resolvePort,
  stateDirFor,
} from '../../dist/engine/session-registry.mjs';

const skill = join(import.meta.dirname, '../..');
const FAKE = join(skill, 'tests/fixtures/fake-chrome.mjs');
const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-cdp-active-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

const freePort = () =>
  new Promise(resolve => {
    const server = createServer().listen(0, () => {
      const { port } = server.address();
      server.close(() => resolve(String(port)));
    });
  });
const cli = (cwd, args) =>
  spawnSync(process.execPath, [join(skill, 'dist/engine/cli.mjs'), ...args], {
    cwd,
    encoding: 'utf8',
    timeout: 20000,
  });
function workspace(name) {
  const cwd = join(root, name);
  mkdirSync(stateDirFor(cwd), { recursive: true });
  return cwd;
}
const record = (cwd, port, startedAt) =>
  writeFileSync(
    join(stateDirFor(cwd), `cdp-session-${port}.json`),
    JSON.stringify({ pid: 1, port, startedAt, isolated: true })
  );
async function fakeBrowser(port) {
  const child = spawn(
    process.execPath,
    [FAKE, `--remote-debugging-port=${port}`],
    { stdio: ['ignore', 'pipe', 'inherit'] }
  );
  await new Promise((resolve, reject) => {
    child.stdout.once('data', resolve);
    child.once('exit', reject);
  });
  return child;
}
const nobody = async () => null;
const only9301 = async port =>
  port === '9301' ? { Browser: 'x', webSocketDebuggerUrl: 'ws://x' } : null;
const stop = child =>
  new Promise(resolve => {
    child.once('exit', resolve);
    child.kill();
  });

test('a command without a port uses the newest live recorded session, an explicit port wins', async () => {
  const cwd = workspace('newest');
  const [live, dead] = [await freePort(), await freePort()];
  const browser = await fakeBrowser(live);
  try {
    record(cwd, live, 1000);
    record(cwd, dead, 2000); // newer, but nothing answers: skipped
    const started = cli(cwd, ['session', '--action', 'start']);
    assert.equal(started.status, 0, started.stderr);
    const { session } = JSON.parse(started.stdout);
    assert.equal(session.browser.endpoint, `http://127.0.0.1:${live}`);
    const explicit = cli(cwd, ['session', '--action', 'start', '--port', dead]);
    assert.notEqual(explicit.status, 0);
    assert.match(explicit.stderr, new RegExp(`port ${dead}`));
  } finally {
    await stop(browser);
  }
});

test('resolvePort falls back to 9222 with no live session and reports its source', async () => {
  const cwd = workspace('fallback');
  record(cwd, '9301', 5);
  assert.deepEqual(await resolvePort(undefined, stateDirFor(cwd), nobody), {
    port: DEFAULT_PORT,
    source: 'default',
  });
  assert.deepEqual(await resolvePort(undefined, stateDirFor(cwd), only9301), {
    port: '9301',
    source: 'recorded',
  });
  assert.deepEqual(await resolvePort('9400', stateDirFor(cwd), only9301), {
    port: '9400',
    source: 'explicit',
  });
  assert.deepEqual(
    (await liveSessions(stateDirFor(cwd), only9301)).map(s => s.port),
    ['9301']
  );
});

test('a missing browser reports the cause and the live session of this workspace', async () => {
  const cwd = workspace('probe-error');
  const [live, dead] = [await freePort(), await freePort()];
  const browser = await fakeBrowser(live);
  try {
    record(cwd, live, 1000);
    const result = cli(cwd, ['targets', '--port', dead]);
    assert.notEqual(result.status, 0);
    assert.match(
      result.stderr,
      new RegExp(`No Chrome on port ${dead} \\(connection refused\\)`)
    );
    assert.match(
      result.stderr,
      new RegExp(`runs on ${live}: pass connection.port ${live}, or call open`)
    );
    assert.doesNotMatch(result.stderr, /open-browser\.mjs/);
  } finally {
    await stop(browser);
  }
});

test('probe failures name refused, timed out and HTTP status causes', () => {
  const refused = new TypeError('fetch failed', {
    cause: Object.assign(new Error('connect'), { code: 'ECONNREFUSED' }),
  });
  assert.equal(probeFailure(refused), 'connection refused');
  assert.equal(
    probeFailure(Object.assign(new Error('x'), { name: 'TimeoutError' })),
    'timed out'
  );
  assert.equal(
    probeFailure(new Error('CDP HTTP 503 for /json/version')),
    'HTTP status 503'
  );
  assert.match(noChromeMessage('9222', 'timed out', []), /Call open/);
});
