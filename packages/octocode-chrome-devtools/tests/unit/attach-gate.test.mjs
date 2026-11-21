// open on a port where a browser this package did not launch is listening needs consent (attach).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';
import { findChrome } from '../../dist/engine/browser-process.mjs';

const skill = join(import.meta.dirname, '../..');
const FAKE = join(skill, 'tests/fixtures/fake-chrome.mjs');
const OPEN = join(skill, 'dist/engine/open-browser.mjs');
const root = realpathSync(mkdtempSync(join(tmpdir(), 'octo-attach-gate-')));
test.after(() => rmSync(root, { recursive: true, force: true }));

const open = (port, ...args) =>
  spawnSync(process.execPath, [OPEN, '--port', port, ...args], {
    cwd: root,
    encoding: 'utf8',
    timeout: 60000,
  });
const lastLine = result => JSON.parse(result.stdout.trim().split('\n').pop());
const stop = child =>
  new Promise(resolve => {
    if (child.exitCode !== null) return resolve();
    child.once('exit', resolve);
    child.kill();
  });
const tabsOf = async port =>
  (await fetch(`http://127.0.0.1:${port}/json`)).json();

async function foreign(port) {
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

test('a listener this package did not launch is refused without attach and untouched', async () => {
  const port = await freePort();
  const browser = await foreign(port);
  try {
    const refused = open(port, '--url', 'http://example.test/x');
    assert.notEqual(refused.status, 0);
    assert.match(
      refused.stderr + refused.stdout,
      new RegExp(
        `A browser this package did not launch is listening on port ${port}\\. Attaching exposes its logged-in pages and cookies: ask the user, then retry with attach: true and acknowledge: true\\.`
      )
    );
    assert.deepEqual(await tabsOf(port), [], 'no tab was opened');
    const unacknowledged = open(port, '--attach');
    assert.notEqual(unacknowledged.status, 0);
    assert.match(unacknowledged.stdout, /--acknowledge/);
    assert.deepEqual(await tabsOf(port), [], 'no tab was opened');
    const attached = open(port, '--attach', '--acknowledge');
    assert.equal(attached.status, 0, attached.stderr);
    assert.equal(lastLine(attached).attached, true);
  } finally {
    await stop(browser);
  }
});

const chrome = findChrome();
test(
  'real Chrome started outside the package: refused, then attached; a package-launched browser reopens without attach',
  { skip: !chrome && 'no Chrome installed', timeout: 180000 },
  async () => {
    const profile = mkdtempSync(join(tmpdir(), 'octo-foreign-profile-'));
    const foreignPort = await freePort();
    const ownPort = await freePort();
    const browser = spawn(
      chrome,
      [
        `--remote-debugging-port=${foreignPort}`,
        `--user-data-dir=${profile}`,
        '--headless=new',
        '--no-first-run',
        '--no-sandbox',
        'about:blank',
      ],
      { stdio: 'ignore' }
    );
    try {
      for (let i = 0; i < 60; i++) {
        if (
          await fetch(`http://127.0.0.1:${foreignPort}/json/version`).then(
            r => r.ok,
            () => false
          )
        )
          break;
        await new Promise(resolve => setTimeout(resolve, 500));
      }
      const refused = open(foreignPort, '--url', 'about:blank#new');
      assert.notEqual(refused.status, 0);
      assert.match(refused.stderr + refused.stdout, /did not launch/);
      assert.ok(
        !(await tabsOf(foreignPort)).some(tab => tab.url.endsWith('#new')),
        'the refused open created no tab'
      );
      const attached = open(foreignPort, '--attach', '--acknowledge');
      assert.equal(attached.status, 0, attached.stderr);
      const reply = lastLine(attached);
      assert.equal(reply.attached, true);
      assert.equal(reply.reused, true);
      assert.doesNotMatch(reply.profile, /launched by octocode/);

      const launched = open(ownPort, '--headless');
      assert.equal(launched.status, 0, launched.stderr);
      const reopened = open(ownPort, '--headless');
      assert.equal(reopened.status, 0, reopened.stderr);
      assert.equal(lastLine(reopened).reused, true);
      assert.equal(lastLine(reopened).isolated, true);
      const viaAttach = lastLine(open(ownPort, '--attach', '--acknowledge'));
      assert.equal(viaAttach.attached, true);
    } finally {
      open(ownPort, '--cleanup');
      await stop(browser);
      rmSync(profile, { recursive: true, force: true });
    }
  }
);

test('through MCP the agent reads the attach refusal itself, not a bare exit code', async () => {
  const { connectStdio } = await import('../../dist/runtime.js');
  const port = Number(await freePort());
  const browser = await foreign(port);
  const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-attach-mcp-')));
  const client = await connectStdio({
    command: process.execPath,
    args: [join(skill, 'bin/octocode-chrome-devtools.mjs')],
    cwd,
  });
  try {
    const reply = await client.callTool({ name: 'open', arguments: { port } });
    const body = (reply.content ?? []).map(part => part.text ?? '').join('');
    assert.equal(reply.isError, true);
    assert.match(body, /did not launch is listening on port/);
    assert.match(body, /attach: true and acknowledge: true/);
    assert.ok(body.length < 900, `refusal is ${body.length} chars`);
  } finally {
    await client.close();
    await stop(browser);
    rmSync(cwd, { recursive: true, force: true });
  }
});
