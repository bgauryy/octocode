import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn, spawnSync } from 'node:child_process';
import { promisify } from 'node:util';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const run = promisify(execFile);
const skill = join(import.meta.dirname, '../..');
const FAKE = join(skill, 'tests/fixtures/fake-chrome.mjs');
const STATE = ['.octocode', 'octocode-chrome-devtools', 'tmp', 'browser-state'];

const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-cdp-cleanup-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

const alive = pid => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};
function workspace(name) {
  const dir = join(root, name);
  mkdirSync(dir, { recursive: true });
  return dir;
}
const paths = (ws, port) => {
  const state = join(ws, ...STATE);
  return {
    state,
    profileDir: join(state, `cdp-chrome-profile-${port}`),
    sessionFile: join(state, `cdp-session-${port}.json`),
  };
};
// Launches a fake browser on port with profileDir, records a session in its
// workspace unless session is false, and returns the process.
async function fakeBrowser(port, profileDir, session) {
  mkdirSync(profileDir, { recursive: true });
  const child = spawn(
    process.execPath,
    [FAKE, `--remote-debugging-port=${port}`, `--user-data-dir=${profileDir}`],
    { stdio: ['ignore', 'pipe', 'inherit'] }
  );
  await new Promise((resolve, reject) => {
    child.stdout.once('data', resolve);
    child.once('exit', reject);
  });
  if (session)
    writeFileSync(
      session.file,
      JSON.stringify({
        pid: session.pid ?? child.pid,
        port,
        profileDir,
        headless: true,
        isolated: true,
        startedAt: Date.now(),
      })
    );
  return child;
}
// Async, so this process reaps exited fake browsers instead of leaving zombies.
const cli = async (cwd, args) => {
  const { stdout } = await run(
    process.execPath,
    [join(skill, 'dist/engine/cli.mjs'), ...args],
    { cwd, encoding: 'utf8', timeout: 15000 }
  );
  return JSON.parse(stdout.trim().split('\n').at(-1));
};
async function exited(pid) {
  for (let i = 0; i < 50 && alive(pid); i++)
    await new Promise(r => setTimeout(r, 100));
  return !alive(pid);
}

test('cleanup from another cwd stops the isolated session of the owning workspace', async () => {
  const port = await freePort();
  const owner = workspace('owner with space');
  const other = workspace('other');
  const { profileDir, sessionFile } = paths(owner, port);
  const browser = await fakeBrowser(port, profileDir, { file: sessionFile });
  try {
    const reused = await cli(other, ['open', '--port', port]);
    assert.equal(reused.reused, true);
    assert.equal(reused.workspace, owner);
    assert.equal(reused.sessionFile, sessionFile);

    const preview = await cli(other, ['cleanup', '--port', port, '--dry-run']);
    assert.equal(preview.status, 'CLEANUP_DRY_RUN');
    assert.equal(preview.wouldKill, true);
    assert.equal(preview.workspace, owner);
    assert.equal(preview.discoveredFromListener, true);
    assert(alive(browser.pid));

    const cleaned = await cli(other, ['cleanup', '--port', port]);
    assert.equal(cleaned.status, 'CLEANED_UP');
    assert.equal(cleaned.workspace, owner);
    assert.equal(cleaned.sessionFile, sessionFile);
    assert.equal(cleaned.pid, browser.pid);
    assert(await exited(browser.pid));
    assert(!existsSync(profileDir));
    assert(!existsSync(sessionFile));
    assert(!existsSync(join(other, '.octocode')), 'cleanup creates no state');
  } finally {
    browser.kill('SIGKILL');
  }
});

test('cleanup in the launching cwd keeps working', async () => {
  const port = await freePort();
  const owner = workspace('same');
  const { profileDir, sessionFile } = paths(owner, port);
  const browser = await fakeBrowser(port, profileDir, { file: sessionFile });
  try {
    const cleaned = await cli(owner, ['cleanup', '--port', port]);
    assert.equal(cleaned.status, 'CLEANED_UP');
    assert.equal(cleaned.workspace, owner);
    assert.equal(cleaned.discoveredFromListener, undefined);
    assert(await exited(browser.pid));
    assert(!existsSync(profileDir));
  } finally {
    browser.kill('SIGKILL');
  }
});

test('cleanup never stops a listener this package did not launch and record', async () => {
  const port = await freePort();
  const other = workspace(`other-${port}`);
  // A user profile outside any package state directory.
  const user = await fakeBrowser(port, join(root, `user-profile-${port}`));
  try {
    for (const args of [
      ['cleanup', '--port', port],
      ['cleanup', '--port', port, '--dry-run'],
    ]) {
      const result = await cli(other, args);
      assert.equal(result.status, 'NO_TRACKED_SESSION');
      assert.equal(result.reason, 'NOT_PACKAGE_PROFILE');
      assert.equal(result.listenerPid, user.pid);
    }
    assert(alive(user.pid));
  } finally {
    user.kill('SIGKILL');
  }

  // A package-shaped profile without its session file.
  const port2 = await freePort();
  const unrecorded = workspace(`unrecorded-${port2}`);
  const browser = await fakeBrowser(port2, paths(unrecorded, port2).profileDir);
  try {
    const result = await cli(other, ['cleanup', '--port', port2]);
    assert.equal(result.status, 'NO_TRACKED_SESSION');
    assert.equal(result.reason, 'NO_SESSION_FILE');
    assert.equal(result.workspace, unrecorded);
    assert(alive(browser.pid));
    assert(existsSync(paths(unrecorded, port2).profileDir));
  } finally {
    browser.kill('SIGKILL');
  }
});

test('a recorded pid that is not the listener is stale and never killed', async () => {
  const port = await freePort();
  const owner = workspace(`stale-${port}`);
  const other = workspace(`stale-other-${port}`);
  const { profileDir, sessionFile } = paths(owner, port);
  const browser = await fakeBrowser(port, profileDir, {
    file: sessionFile,
    pid: process.pid,
  });
  try {
    const preview = await cli(other, ['cleanup', '--port', port, '--dry-run']);
    assert.equal(preview.status, 'CLEANUP_DRY_RUN');
    assert.equal(preview.wouldKill, false);
    const result = await cli(other, ['cleanup', '--port', port]);
    assert.equal(result.status, 'STALE_SESSION_REMOVED');
    assert.equal(result.workspace, owner);
    assert(!existsSync(sessionFile));
    assert(alive(browser.pid));
    assert(existsSync(profileDir));
  } finally {
    browser.kill('SIGKILL');
  }
});

test('cleanup without a listener or session reports no tracked session', async () => {
  // A suite running beside this one can take a freed port before the call, so
  // another port gets another try. A wrong answer repeats on every port.
  let result;
  for (
    let attempt = 0;
    attempt < 5 && result?.reason !== 'NO_LISTENER';
    attempt++
  ) {
    const port = await freePort();
    result = await cli(workspace(`empty-${port}`), ['cleanup', '--port', port]);
  }
  assert.equal(result.status, 'NO_TRACKED_SESSION');
  assert.equal(result.reason, 'NO_LISTENER');
});

test('the launcher rejects ports that are not valid TCP ports', () => {
  const cwd = workspace('ports');
  for (const port of ['0', '65536', '../x']) {
    const result = spawnSync(
      process.execPath,
      [
        join(skill, 'dist/engine/open-browser.mjs'),
        '--port',
        port,
        '--cleanup',
      ],
      { cwd, encoding: 'utf8' }
    );
    assert.equal(result.status, 1, port);
    assert.match(result.stdout, /--port needs a positive integer/);
  }
  assert(!existsSync(join(cwd, '.octocode')));
});
