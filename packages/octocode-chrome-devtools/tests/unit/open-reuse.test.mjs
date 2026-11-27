import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const skill = join(import.meta.dirname, '../..');
const FAKE = join(skill, 'tests/fixtures/fake-chrome.mjs');
const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-open-reuse-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

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
const open = (port, args, env = {}) =>
  spawnSync(
    process.execPath,
    [
      join(skill, 'dist/engine/open-browser.mjs'),
      '--port',
      port,
      '--attach',
      '--acknowledge',
      ...args,
    ],
    {
      cwd: root,
      encoding: 'utf8',
      timeout: 20000,
      env: { ...process.env, ...env },
    }
  );
const lastLine = result => JSON.parse(result.stdout.trim().split('\n').pop());
const json = async (port, path, init) =>
  (await fetch(`http://127.0.0.1:${port}${path}`, init)).json();
const newTab = (port, url) =>
  json(port, `/json/new?${encodeURIComponent(url)}`, { method: 'PUT' });
const stop = child =>
  new Promise(resolve => {
    child.once('exit', resolve);
    child.kill();
  });

test('open --url on a reused browser opens the url instead of ignoring it', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    const url = 'http://example.test/landing?a=1';
    const result = spawnSync(
      process.execPath,
      [
        join(skill, 'dist/engine/open-browser.mjs'),
        '--port',
        port,
        '--attach',
        '--acknowledge',
        '--url',
        url,
      ],
      { cwd: root, encoding: 'utf8', timeout: 20000 }
    );
    assert.equal(result.status, 0, result.stderr);
    const opened = JSON.parse(result.stdout.trim().split('\n').pop());
    assert.equal(opened.reused, true);
    const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    assert.deepEqual(
      tabs.map(tab => tab.url),
      [url]
    );
    assert.equal(opened.targetId, tabs[0].id);
    assert.equal(opened.url, url);
  } finally {
    await stop(browser);
  }
});

test('open without --url on a reused browser opens no tab', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    const result = spawnSync(
      process.execPath,
      [
        join(skill, 'dist/engine/open-browser.mjs'),
        '--port',
        port,
        '--attach',
        '--acknowledge',
      ],
      { cwd: root, encoding: 'utf8', timeout: 20000 }
    );
    assert.equal(result.status, 0, result.stderr);
    const opened = JSON.parse(result.stdout.trim().split('\n').pop());
    assert.equal(opened.reused, true);
    assert.equal(opened.targetId, undefined);
    const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    assert.deepEqual(tabs, []);
  } finally {
    await stop(browser);
  }
});

const STEALTH = { CDP_STEALTH: '1', CDP_SKIP_STEALTH_VERIFY: '1' };

test('with stealth on, open --url loads the URL only after the stealth setup', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    const url = 'http://example.test/guarded';
    const result = open(port, ['--url', url], STEALTH);
    assert.equal(result.status, 0, result.stderr);
    // stdout carries the result line only; stealth progress goes to stderr.
    assert.equal(result.stdout.trim().split('\n').length, 1, result.stdout);
    const opened = lastLine(result);
    const log = await json(port, '/__log');
    assert.deepEqual(
      log.filter(entry => entry.kind === 'new').map(entry => entry.url),
      ['about:blank']
    );
    const methods = log.filter(entry => entry.kind === 'cdp');
    const stealth = methods.findIndex(
      entry => entry.method === 'Network.setUserAgentOverride'
    );
    const navigate = methods.findIndex(
      entry => entry.method === 'Page.navigate' && entry.url === url
    );
    assert.ok(stealth >= 0, 'stealth setup ran on the new tab');
    assert.ok(navigate > stealth, 'first document load follows stealth setup');
    assert.deepEqual(
      (await json(port, '/json')).map(tab => tab.url),
      [url]
    );
    assert.equal(opened.url, url);
  } finally {
    await stop(browser);
  }
});

test('with stealth off, open --url still opens about:blank and then navigates', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    const url = 'http://example.test/plain';
    assert.equal(open(port, ['--url', url]).status, 0);
    const log = await json(port, '/__log');
    assert.deepEqual(
      log.filter(entry => entry.kind === 'new').map(entry => entry.url),
      ['about:blank']
    );
    assert.ok(
      !log.some(entry => entry.method === 'Network.setUserAgentOverride')
    );
    assert.ok(
      log.some(entry => entry.method === 'Page.navigate' && entry.url === url)
    );
  } finally {
    await stop(browser);
  }
});

const PLAN = JSON.stringify({
  steps: [
    {
      op: 'cdp',
      method: 'Runtime.evaluate',
      acknowledge: true,
      params: { expression: 'document.title' },
    },
  ],
});
const plan = port =>
  spawnSync(
    process.execPath,
    [join(skill, 'dist/engine/cli.mjs'), 'run', '--json', PLAN, '--port', port],
    { cwd: root, encoding: 'utf8', timeout: 20000 }
  );
// The tabs a plan touched: page sockets that received a Runtime.evaluate after `from`.
const evaluated = (log, from) => [
  ...new Set(
    log
      .slice(from)
      .filter(entry => entry.method === 'Runtime.evaluate')
      .map(entry => entry.tab)
  ),
];

test('a plan without --target acts on the tab open --url made, and other tabs keep their URLs', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    const mine = 'http://example.test/mine';
    const others = ['http://other.test/one', 'http://other.test/two'];
    for (const url of others) await newTab(port, url);
    const opened = lastLine(open(port, ['--url', mine]));
    // The user opens another tab afterwards: it is newest, but not the selected one.
    await newTab(port, 'http://other.test/three');
    const before = (await json(port, '/__log')).length;
    const result = plan(port);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(evaluated(await json(port, '/__log'), before), [
      opened.targetId,
    ]);
    const urls = Object.fromEntries(
      (await json(port, '/json')).map(tab => [tab.id, tab.url])
    );
    assert.equal(urls[opened.targetId], mine);
    assert.deepEqual(
      Object.values(urls).toSorted(),
      [...others, mine, 'http://other.test/three'].toSorted()
    );
  } finally {
    await stop(browser);
  }
});

test('open --url brings the new tab to the front of the target list', async () => {
  const port = await freePort();
  const browser = await fakeBrowser(port);
  try {
    await newTab(port, 'http://other.test/one');
    const opened = lastLine(open(port, ['--url', 'http://example.test/x']));
    assert.equal((await json(port, '/json'))[0].id, opened.targetId);
    const log = await json(port, '/__log');
    assert.ok(log.some(entry => entry.kind === 'activate'));
  } finally {
    await stop(browser);
  }
});

const page = (id, url) => ({ id, type: 'page', url, title: '' });
const endpoint = targets => ({
  targets: async () => targets,
  activate: async () => {},
});
const quiet = () => {};

test('default page selection prefers the opened tab while it exists, then falls back', async () => {
  const { selectTarget } =
    await import('../../dist/engine/cdp-runner-targets.mjs');
  const tabs = [page('a', 'http://a.test/'), page('b', 'http://b.test/')];
  const strict = { strict: true };
  await assert.rejects(
    selectTarget(endpoint(tabs), strict, quiet),
    /Ambiguous target/
  );
  const chosen = await selectTarget(
    endpoint(tabs),
    { ...strict, preferredId: 'b' },
    quiet
  );
  assert.equal(chosen.targetInfo.id, 'b');
  await assert.rejects(
    selectTarget(endpoint(tabs), { ...strict, preferredId: 'gone' }, quiet),
    /Ambiguous target/
  );
  const explicit = await selectTarget(
    endpoint(tabs),
    { ...strict, preferredId: 'b', targetId: 'a' },
    quiet
  );
  assert.equal(explicit.targetInfo.id, 'a');
});
