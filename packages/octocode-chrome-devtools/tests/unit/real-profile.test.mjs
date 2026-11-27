import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const skill = join(import.meta.dirname, '../..');
const engine = join(skill, 'dist/engine');
const FAKE = join(skill, 'tests/fixtures/fake-chrome.mjs');
const {
  browserFamily,
  realProfileDataDir,
  remoteDebuggingBlocked,
  dataDirInUse,
  planRealProfile,
} = await import(join(engine, 'real-profile.mjs'));

const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-real-profile-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

test('browser family follows the executable, so each browser opens its own data dir', () => {
  const cases = {
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome': 'chrome',
    '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary':
      'chrome-canary',
    '/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing':
      'chrome-for-testing',
    '/Applications/Chromium.app/Contents/MacOS/Chromium': 'chromium',
    '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser': 'brave',
    'google-chrome': 'chrome',
    'chromium-browser': 'chromium',
    'C:\\Users\\u\\AppData\\Local\\Chromium\\Application\\chrome.exe':
      'chromium',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe': 'chrome',
    '/opt/firefox/firefox': null,
  };
  for (const [path, family] of Object.entries(cases))
    assert.equal(browserFamily(path), family, path);
  const env = { HOME: '/h', LOCALAPPDATA: 'C:\\L' };
  assert.equal(
    realProfileDataDir('chromium', 'darwin', env),
    '/h/Library/Application Support/Chromium'
  );
  assert.equal(
    realProfileDataDir('brave', 'linux', env),
    '/h/.config/BraveSoftware/Brave-Browser'
  );
  assert.notEqual(
    realProfileDataDir('chrome', 'darwin', env),
    realProfileDataDir('brave', 'darwin', env)
  );
});

test('Google Chrome 136+ refuses remote debugging on its real profile; Chromium does not', () => {
  assert.equal(remoteDebuggingBlocked('chrome', 136), true);
  assert.equal(remoteDebuggingBlocked('chrome-canary', 150), true);
  assert.equal(remoteDebuggingBlocked('chrome', 135), false);
  assert.equal(remoteDebuggingBlocked('chrome', null), false);
  assert.equal(remoteDebuggingBlocked('chromium', 154), false);
  assert.equal(remoteDebuggingBlocked('chrome-for-testing', 154), false);
});

test(
  'a live SingletonLock owner marks the data dir in use; a stale one does not',
  { skip: process.platform === 'win32' },
  () => {
    const dir = join(root, 'lock');
    mkdirSync(dir, { recursive: true });
    assert.equal(dataDirInUse(dir, process.platform), false);
    symlinkSync(`host-${process.pid}`, join(dir, 'SingletonLock'));
    assert.equal(dataDirInUse(dir, process.platform), true);
    rmSync(join(dir, 'SingletonLock'));
    symlinkSync('host-999999999', join(dir, 'SingletonLock'));
    assert.equal(dataDirInUse(dir, process.platform), false);
  }
);

function home(name) {
  const dir = join(root, name);
  const data = realProfileDataDir('chromium', process.platform, {
    HOME: dir,
    LOCALAPPDATA: dir,
  });
  mkdirSync(join(data, 'Default'), { recursive: true });
  writeFileSync(join(data, 'Default', 'Preferences'), '{}');
  return { dir, data };
}

test('planRealProfile never creates a profile or shares a held data dir', () => {
  const { dir, data } = home('plan');
  const env = { HOME: dir, LOCALAPPDATA: dir };
  const plan = opts =>
    planRealProfile({
      chromePath: '/x/Chromium',
      profile: 'Default',
      os: process.platform,
      env,
      major: () => 154,
      ...opts,
    });
  assert.deepEqual(plan(), { userDataDir: data, family: 'chromium' });
  assert.match(
    plan({ profile: 'Profile 9' }).error,
    /not found.*available: Default.*Refusing to create/s
  );
  assert.match(
    plan({ chromePath: '/x/Google Chrome' }).error,
    /136\+ ignores --remote-debugging-port/
  );
  assert.match(
    plan({ chromePath: '/x/firefox' }).error,
    /Cannot locate the real profile/
  );
  if (process.platform !== 'win32') {
    symlinkSync(`host-${process.pid}`, join(data, 'SingletonLock'));
    assert.match(plan().error, /is running and holds/);
    rmSync(join(data, 'SingletonLock'));
  }
  assert.equal(existsSync(join(data, 'Profile 9')), false);
});

// open-browser end to end with a browser-free stand-in named like the real executable.
function fakeBrowser(name) {
  const bin = join(root, 'bin', name);
  mkdirSync(join(root, 'bin'), { recursive: true });
  writeFileSync(
    bin,
    `#!/bin/sh\nif [ "$1" = "--version" ]; then echo "${name} 154.0.8037.98"; exit 0; fi\nexec "${process.execPath}" "${FAKE}" "$@"\n`
  );
  chmodSync(bin, 0o755);
  return bin;
}

function openBrowser(args, env) {
  const result = spawnSync(
    process.execPath,
    [join(engine, 'open-browser.mjs'), ...args],
    {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, ...env, XDG_CONFIG_HOME: '' },
      timeout: 30000,
    }
  );
  const line = result.stdout.trim().split('\n').pop();
  return {
    status: result.status,
    json: JSON.parse(line),
    stderr: result.stderr,
  };
}

const killPort = port =>
  spawnSync('pkill', ['-f', `remote-debugging-port=${port}`]);

test(
  'open --profile launches the browser on its own data dir and cleanup leaves it intact',
  { skip: process.platform === 'win32' },
  async () => {
    const { dir, data } = home('launch');
    const port = await freePort();
    const env = { HOME: dir };
    try {
      const opened = openBrowser(
        [
          '--port',
          port,
          '--profile',
          'Default',
          '--chromePath',
          fakeBrowser('Chromium'),
        ],
        env
      );
      assert.equal(opened.status, 0, opened.stderr);
      assert.equal(opened.json.status, 'BROWSER_READY');
      assert.equal(opened.json.isolated, false);
      assert.equal(opened.json.sessionFile, null);
      const cleaned = openBrowser(['--port', port, '--cleanup'], env);
      assert.notEqual(cleaned.json.status, 'CLEANED_UP');
      assert.equal(existsSync(join(data, 'Default', 'Preferences')), true);
    } finally {
      killPort(port);
    }
  }
);

test(
  'open --profile fails clearly instead of silently using an isolated profile',
  { skip: process.platform === 'win32' },
  async () => {
    const { dir, data } = home('refuse');
    const port = await freePort();
    const env = { HOME: dir };
    const chromium = fakeBrowser('Chromium');
    symlinkSync(`host-${process.pid}`, join(data, 'SingletonLock'));
    const held = openBrowser(
      ['--port', port, '--profile', 'Default', '--chromePath', chromium],
      env
    );
    assert.equal(held.status, 1);
    assert.match(held.json.message, /is running and holds/);
    rmSync(join(data, 'SingletonLock'));

    const chrome = openBrowser(
      [
        '--port',
        port,
        '--profile',
        'Default',
        '--chromePath',
        fakeBrowser('Google Chrome'),
      ],
      env
    );
    assert.equal(chrome.status, 1);
    assert.match(chrome.json.message, /136\+ ignores --remote-debugging-port/);

    const headless = openBrowser(
      [
        '--port',
        port,
        '--profile',
        'Default',
        '--headless',
        '--chromePath',
        chromium,
      ],
      env
    );
    assert.equal(headless.status, 1);
    assert.match(headless.json.message, /drop --headless/);
    killPort(port);
  }
);
