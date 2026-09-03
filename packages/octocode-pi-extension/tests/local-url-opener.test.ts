import assert from 'node:assert/strict';
import { test } from 'vitest';
import { openLocalUrl } from '../src/tools/local-url-opener.js';

const URL = 'http://127.0.0.1:4321/design/';

test('openLocalUrl rejects non-loopback URLs before launching anything', async () => {
  let launched = false;
  const result = await openLocalUrl('https://example.com/', {
    launch: async () => { launched = true; },
  });
  assert.equal(result.ok, false);
  assert.equal(result.openedIn, 'none');
  assert.equal(launched, false);
  assert.match(result.message ?? '', /loopback/i);
});

test('openLocalUrl uses the VS Code integrated browser when its host API is available', async () => {
  let chromeChecked = false;
  const result = await openLocalUrl(URL, {
    env: { TERM_PROGRAM: 'vscode' },
    openInVsCode: async (url) => url === URL,
    getChromeInstallations: async () => { chromeChecked = true; return []; },
  });
  assert.equal(result.ok, true);
  assert.equal(result.openedIn, 'vscode');
  assert.equal(chromeChecked, false);
});

test('openLocalUrl discovers Chrome without starting an automation profile', async () => {
  const calls: Array<{ command: string; args: string[] }> = [];
  const result = await openLocalUrl(URL, {
    getChromeInstallations: async () => ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'],
    launch: async (command, args) => { calls.push({ command, args }); },
  });
  assert.equal(result.ok, true);
  assert.equal(result.openedIn, 'chrome');
  assert.deepEqual(calls, [{
    command: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    args: [URL],
  }]);
});

test('openLocalUrl falls back to the platform opener when Chrome is unavailable', async () => {
  const calls: Array<{ command: string; args: string[] }> = [];
  const result = await openLocalUrl(URL, {
    platform: 'darwin',
    getChromeInstallations: async () => [],
    launch: async (command, args) => { calls.push({ command, args }); },
  });
  assert.equal(result.ok, true);
  assert.equal(result.openedIn, 'system');
  assert.deepEqual(calls, [{ command: 'open', args: [URL] }]);
});

test('explicit browser choices are honored and none has no side effects', async () => {
  let launches = 0;
  const none = await openLocalUrl(URL, {
    preference: 'none',
    launch: async () => { launches += 1; },
  });
  assert.equal(none.ok, true);
  assert.equal(none.openedIn, 'none');

  const chrome = await openLocalUrl(URL, {
    preference: 'chrome',
    getChromeInstallations: async () => [],
    launch: async () => { launches += 1; },
  });
  assert.equal(chrome.ok, false);
  assert.equal(chrome.openedIn, 'none');
  assert.equal(launches, 0);
});

test('auto mode reports a useful failure when every opener fails', async () => {
  const result = await openLocalUrl(URL, {
    platform: 'linux',
    getChromeInstallations: async () => [],
    launch: async () => { throw new Error('missing executable'); },
  });
  assert.equal(result.ok, false);
  assert.equal(result.openedIn, 'none');
  assert.match(result.message ?? '', /missing executable/);
  assert.match(result.message ?? '', /127\.0\.0\.1/);
});
