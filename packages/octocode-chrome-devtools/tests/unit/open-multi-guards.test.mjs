// Origin allowlist, attach and noSandbox invariants that need no Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync, statSync } from 'node:fs';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const pkg = join(import.meta.dirname, '../..');
const bin = join(pkg, 'bin/octocode-chrome-devtools.mjs');
const { validatePlan } = await import(
  join(pkg, 'dist/engine/cdp-checks/plan-validate.mjs')
);
const meta = await import(join(pkg, 'dist/engine/session-meta.mjs'));
const { chromeLaunchArgs, isNoSandboxFailure, NO_SANDBOX_HINT } = await import(
  join(pkg, 'dist/engine/browser-launch-config.mjs')
);

const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-open-multi-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

const listening = port =>
  new Promise(resolve => {
    const socket = createConnection(Number(port), '127.0.0.1');
    socket.once('connect', () => (socket.destroy(), resolve(true)));
    socket.once('error', () => resolve(false));
  });
const engine = (...args) =>
  spawnSync(process.execPath, [bin, '/raw', ...args], {
    cwd: root,
    encoding: 'utf8',
    timeout: 60000,
  });

const ALLOWED = ['https://app.example.com'];
// The origins a message names, parsed; sentence punctuation after one is not part of it.
const origins = text =>
  new Set(
    (text.match(/https?:\/\/\S*[^\s.,;:)]/g) ?? []).map(
      link => new URL(link).origin
    )
  );

test('validatePlan refuses a goto or after.url outside the list and names both origins', () => {
  const plan = url => ({ steps: [{ op: 'goto', url }] });
  assert.doesNotThrow(() =>
    validatePlan(plan('https://app.example.com/x?y=1'), {
      allowOrigins: ALLOWED,
    })
  );
  assert.doesNotThrow(() =>
    validatePlan(plan('about:blank'), { allowOrigins: ALLOWED })
  );
  assert.doesNotThrow(() => validatePlan(plan('https://evil.test/')));
  assert.throws(
    () => validatePlan(plan('https://evil.test/p'), { allowOrigins: ALLOWED }),
    error => {
      const named = origins(error.message);
      return named.has('https://evil.test') && named.has(ALLOWED[0]);
    }
  );
  assert.throws(
    () =>
      validatePlan(
        {
          steps: [
            {
              op: 'wait',
              role: 'button',
              name: 'x',
              after: { url: 'https://other.test/' },
            },
          ],
        },
        { allowOrigins: ALLOWED }
      ),
    /https:\/\/other\.test/
  );
  assert.throws(
    () =>
      validatePlan(
        {
          steps: [
            {
              op: 'cdp',
              method: 'Page.navigate',
              params: { url: 'https://evil.test/' },
            },
          ],
        },
        { allowOrigins: ALLOWED }
      ),
    /https:\/\/evil\.test/
  );
});

test('session meta round-trips through a 0600 file next to the session file and drops damaged fields', () => {
  const port = '45678';
  assert.deepEqual(meta.readSessionMeta(port, root), {});
  meta.updateSessionMeta(
    port,
    { allowOrigins: ALLOWED, groups: { g: ['a', 'b'] } },
    root
  );
  meta.updateSessionMeta(port, { allowOrigins: undefined }, root);
  assert.deepEqual(meta.readSessionMeta(port, root), {
    groups: { g: ['a', 'b'] },
  });
  const file = meta.metaFileIn(
    join(root, '.octocode/octocode-chrome-devtools/tmp/browser-state'),
    port
  );
  assert.equal(statSync(file).mode & 0o777, 0o600);
  meta.removeSessionMeta(port, root);
  assert.deepEqual(meta.readSessionMeta(port, root), {});
});

test('a run to an unlisted origin fails before connecting to any browser', async () => {
  const port = await freePort();
  meta.updateSessionMeta(port, { allowOrigins: ALLOWED }, root);
  const refused = engine(
    'run',
    '--port',
    port,
    '--json',
    JSON.stringify({ steps: [{ op: 'goto', url: 'https://evil.test/' }] })
  );
  assert.notEqual(refused.status, 0);
  const text = refused.stdout + refused.stderr;
  assert.match(text, /https:\/\/evil\.test/);
  assert.match(text, /allowed origins: https:\/\/app\.example\.com/);
  // Nothing listens on the port: reaching the connection step would have said "No Chrome".
  assert.doesNotMatch(text, /No Chrome on port/);
  const tab = engine(
    'snapshot',
    '--port',
    port,
    '--new-tab',
    'https://evil.test/'
  );
  assert.match(tab.stdout + tab.stderr, /not allowed for this session/);
  meta.removeSessionMeta(port, root);
});

test('open --attach without --acknowledge is refused by the engine, naming the field', async () => {
  const port = await freePort();
  const refused = engine('open', '--port', port, '--attach');
  assert.notEqual(refused.status, 0);
  assert.match(refused.stdout, /--acknowledge/);
  assert.match(refused.stdout, /acknowledge: true/);
  assert.equal(await listening(port), false);
});

test('open --attach to a closed port fails naming the port and launches nothing', async () => {
  const port = await freePort();
  const result = engine(
    'open',
    '--port',
    port,
    '--attach',
    '--acknowledge',
    '--headless'
  );
  assert.notEqual(result.status, 0);
  const noLaunch = engine('open', '--port', port, '--attach', '--acknowledge');
  assert.notEqual(noLaunch.status, 0);
  const message = JSON.parse(noLaunch.stdout.trim().split('\n').pop()).message;
  assert.match(message, new RegExp(`port ${port}`));
  assert.match(message, /Nothing was launched/);
  assert.equal(await listening(port), false);
  const noPort = engine('open', '--attach');
  assert.notEqual(noPort.status, 0);
});

test('noSandbox adds the sandbox-off flags on any OS and a sandbox failure says to retry with noSandbox', () => {
  const launch = (os, noSandbox) =>
    chromeLaunchArgs({
      port: '1',
      userDataDir: '/x',
      isolated: true,
      profile: 'Default',
      headless: false,
      os,
      userAgent: '',
      windowSize: '',
      enableFeatures: '',
      proxy: { server: '', bypassList: '', pacUrl: '' },
      url: '',
      noSandbox,
    });
  for (const flag of ['--no-sandbox', '--disable-setuid-sandbox'])
    assert.ok(launch('darwin', true).includes(flag));
  assert.ok(!launch('darwin', false).includes('--no-sandbox'));
  assert.ok(
    isNoSandboxFailure('[1:1:ERROR] No usable sandbox! Update your kernel')
  );
  assert.ok(!isNoSandboxFailure('some other failure'));
  assert.match(NO_SANDBOX_HINT, /noSandbox: true/);
});

test('open --emulation is refused by the engine for a bad value or a missing consent', () => {
  const run = emulation =>
    engine('open', '--port', '9', '--emulation', emulation);
  const bad = run('{"colorScheme":"sepia"}');
  assert.equal(bad.status, 2);
  assert.match(bad.stdout, /colorScheme/);
  const geo = run('{"geolocation":{"latitude":1,"longitude":2}}');
  assert.match(geo.stdout, /--acknowledge/);
  const range = run('{"cpuThrottle":0.5}');
  assert.equal(range.status, 2);
  assert.match(range.stdout, /cpuThrottle/);
});
