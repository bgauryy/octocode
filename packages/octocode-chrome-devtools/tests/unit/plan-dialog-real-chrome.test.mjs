// The plan-level `dialog` option answers alert/confirm dialogs that an act step opens: accept
// reports the dialog as accepted, and the default keeps dismissing it with the JS_DIALOG finding.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';
const page = `<!doctype html><title>Dialog</title><button id=go onclick="alert('x'); document.getElementById('out').textContent='done'">Go</button><p id=out></p>`;

test(
  'dialog accept answers an alert opened by act; the default dismisses it with the finding',
  { skip, timeout: 180000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-plan-dialog-'));
    const web = spawn(
      process.execPath,
      [
        '-e',
        `require('node:http').createServer((_, r) => { r.setHeader('content-type', 'text/html'); r.end(${JSON.stringify(page)}); }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['ignore', 'pipe', 'inherit'] }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const port = await freePort();
    const plan = extra =>
      spawnSync(
        process.execPath,
        [
          bin,
          '/raw',
          'run',
          '--port',
          String(port),
          '--json',
          JSON.stringify({
            waitMs: 8000,
            ...extra,
            steps: [
              { op: 'act', action: 'click', selector: '#go' },
              { op: 'wait', selector: '#out', value: 'done' },
            ],
          }),
        ],
        { cwd, encoding: 'utf8' }
      );
    try {
      const opened = spawnSync(
        process.execPath,
        [bin, '/cli', 'open', '--input', '-', '--json'],
        {
          cwd,
          encoding: 'utf8',
          input: JSON.stringify({
            port,
            headless: true,
            url: `http://127.0.0.1:${webPort}/`,
          }),
        }
      );
      assert.match(opened.stdout, /BROWSER_READY/);
      const dismissed = plan({});
      assert.equal(dismissed.status, 0, dismissed.stdout + dismissed.stderr);
      assert.match(dismissed.stdout, /JS_DIALOG alert "x" dismissed/);
      const accepted = plan({ dialog: 'accept' });
      assert.equal(accepted.status, 0, accepted.stdout + accepted.stderr);
      assert.match(accepted.stdout, /JS_DIALOG alert "x" accepted/);
      assert.doesNotMatch(accepted.stdout, /JS_DIALOG[^\n]*dismissed/);
      const bad = plan({ dialog: 'ignore' });
      assert.notEqual(bad.status, 0);
      assert.match(bad.stdout + bad.stderr, /dialog must be accept or dismiss/);
    } finally {
      spawnSync(
        process.execPath,
        [bin, '/cli', 'cleanup', '--input', '-', '--json'],
        { cwd, input: JSON.stringify({ port }) }
      );
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
