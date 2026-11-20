// A fresh launch URL that redirects outside the allowlist: Chrome navigates it itself, so the
// engine must check where the first page settled, close it and fail (real isolated headless Chrome).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const serverSource = `
require('node:http').createServer((req, res) => {
  const host = req.headers.host.split(':')[1];
  if (req.url === '/redirect') { res.writeHead(302, { location: 'http://localhost:' + host + '/final' }); return res.end(); }
  res.setHeader('content-type', 'text/html');
  res.end('<!doctype html><title>ok</title><h1>OK</h1>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'a launch URL that redirects outside the allowlist fails and its tab is closed',
  { skip, timeout: 120000 },
  async () => {
    const cwd = realpathSync(
      mkdtempSync(join(tmpdir(), 'octo-launch-redirect-'))
    );
    const web = spawn(process.execPath, ['-e', serverSource], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(String(chunk).trim()))
    );
    const base = `http://127.0.0.1:${webPort}`;
    const port = await freePort();
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
        timeout: 90000,
      });
    try {
      const refused = engine(
        'open',
        '--headless',
        '--port',
        port,
        '--allow-origin',
        base,
        '--url',
        `${base}/redirect`
      );
      assert.notEqual(refused.status, 0);
      assert.match(
        refused.stderr + refused.stdout,
        /redirected to .*localhost.*outside the allowlist/
      );
      const pages = (
        await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
      ).filter(t => t.type === 'page' && /localhost/.test(t.url));
      assert.deepEqual(pages, [], 'the redirected tab stays open');
      // The same list lets a URL that stays inside it through.
      const allowed = engine(
        'open',
        '--headless',
        '--port',
        port,
        '--allow-origin',
        base,
        '--url',
        `${base}/`
      );
      assert.equal(allowed.status, 0, allowed.stderr);
    } finally {
      engine('cleanup', '--port', port);
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
