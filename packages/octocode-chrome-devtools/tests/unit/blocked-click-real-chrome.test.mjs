// A click on a button covered by an overlay, against a real isolated headless Chrome (temporary
// workspace, own profile, no network): the plan reports the interception, the counter stays
// at 0 and nothing is replayed; after the overlay is removed a fresh click counts once.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const page = `<button id=b style="position:absolute;left:20px;top:20px;width:120px;height:40px"
  onclick="out.textContent=Number(out.textContent)+1">Go</button><output id=out>0</output>
  <div id=cover style="position:fixed;inset:0;background:rgba(0,0,0,.2)"></div>`;
const url = `data:text/html,${encodeURIComponent(page)}`;

const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });

test(
  'an overlay-blocked click reports the blocker, leaves the counter at 0 and a later click counts once',
  { skip, timeout: 120000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-click-'));
    const port = await freePort();
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      // Failures print the capture on stderr without the structuredContent wrapper.
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    const run = steps => {
      const out = cli('run', {
        connection: { port },
        plan: { waitMs: 1500, steps },
      });
      const result = JSON.parse(
        readFileSync(join(cwd, out.next.steps.query.file), 'utf8')
      );
      return { out, result };
    };
    const counter = () => {
      const { result } = run([
        { op: 'extract', selector: '#out', fields: ['text'] },
      ]);
      return JSON.parse(readFileSync(result.steps[0].artifact, 'utf8'))[0].text;
    };
    try {
      const opened = cli('open', { port, headless: true, url: 'about:blank' });
      assert.equal(opened.ok, true, JSON.stringify(opened));
      assert.equal(opened.data.isolated, true);

      const blocked = run([
        { op: 'goto', url },
        { op: 'act', action: 'click', selector: '#b' },
      ]);
      assert.equal(blocked.out.ok, false);
      assert.match(blocked.result.failure.error, /covered by .*div#cover/);
      assert.equal(blocked.result.steps[1].status, 'failed');
      const evidence = JSON.parse(
        readFileSync(blocked.result.steps[1].artifact, 'utf8')
      );
      assert.equal(evidence.covered, true);
      assert.match(evidence.coveredBy, /div#cover/);
      assert.equal(counter(), '0');

      const retried = run([
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: { expression: "document.getElementById('cover').remove()" },
        },
        { op: 'act', action: 'click', selector: '#b' },
      ]);
      assert.equal(
        retried.out.ok,
        true,
        JSON.stringify(retried.result.failure)
      );
      assert.equal(counter(), '1');
    } finally {
      try {
        cli('cleanup', { port });
      } finally {
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
