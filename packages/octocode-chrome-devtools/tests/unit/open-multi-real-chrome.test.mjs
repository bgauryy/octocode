// Attach and the redirect guard against a real isolated headless
// Chrome (temporary workspace, loopback page server in a child process).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePortText as freePort } from '../fixtures/free-port.mjs';

const pkg = join(import.meta.dirname, '../..');
const bin = join(pkg, 'bin/octocode-chrome-devtools.mjs');
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
  res.end('<!doctype html><title>' + req.url + '</title><h1>PAGE ' + req.url + '</h1>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'attach and the redirect guard work on real Chrome',
  { skip, timeout: 240000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-open-multi-')));
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
        timeout: 120000,
      });
    const last = result => JSON.parse(result.stdout.trim().split('\n').pop());
    try {
      // Redirect leaving the list: the pre-check passes (same origin), the executor stops the step.
      const launched = last(
        engine('open', '--port', port, '--headless', '--allow-origin', base)
      );
      assert.equal(launched.status, 'BROWSER_READY');
      assert.deepEqual(launched.allowOrigins, [base]);
      const refusedUpFront = engine(
        'run',
        '--port',
        port,
        '--json',
        JSON.stringify({ steps: [{ op: 'goto', url: 'http://localhost:1/' }] })
      );
      assert.notEqual(refusedUpFront.status, 0);
      assert.match(
        refusedUpFront.stdout + refusedUpFront.stderr,
        /not allowed/
      );
      const redirect = engine(
        'run',
        '--port',
        port,
        '--json',
        JSON.stringify({
          steps: [{ op: 'goto', url: `${base}/redirect` }],
        })
      );
      assert.notEqual(redirect.status, 0, redirect.stdout);
      assert.match(
        redirect.stdout + redirect.stderr,
        /navigated outside the allowlist to http:\/\/localhost:/
      );

      // The list can be changed on the reused browser.
      const widened = last(
        engine(
          'open',
          '--port',
          port,
          '--allow-origin',
          `${base},http://localhost:${webPort}`
        )
      );
      assert.equal(widened.reused, true);
      assert.equal(widened.allowOrigins.length, 2);

      const plain = last(
        engine('open', '--port', port, '--url', `${base}/plain`)
      );
      assert.ok(plain.targetId);

      // Attach: states the facts, never launches.
      const attached = last(
        engine('open', '--port', port, '--attach', '--acknowledge')
      );
      assert.equal(attached.attached, true);
      assert.match(attached.browserVersion, /Chrome/);
      assert.ok(attached.openTabCount >= 2);
      assert.ok(attached.openTabs.every(url => !url.includes('?')));
      assert.match(attached.profile, /isolated/);

      const cleanup = last(engine('cleanup', '--port', port));
      assert.equal(cleanup.status, 'CLEANED_UP');
    } finally {
      engine('cleanup', '--port', port);
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
