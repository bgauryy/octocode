// network-conditions on a real isolated headless Chrome: a blocked URL reports blocked, a mock
// replaces a response body, offline fails the load, an extra header reaches the server, and a
// bad mock is refused before the page is touched.
import { DIE_WITH_PARENT } from '../fixtures/die-with-parent.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const dist = join(import.meta.dirname, '../../dist/engine');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const RUNNER = `
import { connectCDP } from ${JSON.stringify(join(dist, 'cdp-connection.mjs'))};
const targets = await (await fetch('http://127.0.0.1:' + process.env.PORT + '/json')).json();
const target = targets.find(t => t.type === 'page');
const cdp = await connectCDP(target.webSocketDebuggerUrl, { targetInfo: target });
cdp.outputDir = process.env.OUT;
const recipe = await import(${JSON.stringify(dist)} + '/cdp-checks/' + process.argv[1] + '-check.mjs');
await recipe.run(cdp);
cdp.close();
process.exit(process.exitCode ?? 0);
`;

const REAL = 'REAL-SERVER-BODY';
const MOCK = 'MOCKED-BODY';
// The page reports what it saw to /report; /seen lists every request the server received.
const page = `<!doctype html><title>conditions</title><script>
(async () => {
  const data = await fetch('/api/data').then(r => r.text(), () => 'FETCH-FAILED');
  const blocked = await fetch('/blocked/lib.js').then(r => r.status, () => 'FETCH-FAILED');
  await fetch('/report?data=' + encodeURIComponent(data) + '&blocked=' + blocked);
})();
</script>`;
const serverSource = `
const seen = [];
require('node:http').createServer((req, res) => {
  if (req.url === '/seen') {
    res.setHeader('content-type', 'application/json');
    return res.end(JSON.stringify(seen));
  }
  seen.push({ url: req.url, headers: req.headers });
  if (req.url === '/') {
    res.setHeader('content-type', 'text/html');
    return res.end(${JSON.stringify(page)});
  }
  res.setHeader('content-type', 'text/plain');
  res.end(req.url === '/api/data' ? ${JSON.stringify(REAL)} : 'other');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });
`;

test(
  'network-conditions blocks, mocks, goes offline, adds headers and validates before touching the page',
  { skip, timeout: 150000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-net-cond-'));
    const web = spawn(
      process.execPath,
      [...DIE_WITH_PARENT, '-e', serverSource],
      {
        stdio: ['pipe', 'pipe', 'inherit'],
      }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const origin = `http://127.0.0.1:${webPort}`;
    const seen = async () => (await fetch(`${origin}/seen`)).json();
    const port = await freePort();
    const cli = async (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = await promisify(execFile)(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      ).catch(error => error);
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    const conditions = async options => {
      const outputDir = mkdtempSync(join(cwd, 'out-'));
      const env = Object.fromEntries(
        Object.entries(options).map(([k, v]) => [
          k,
          typeof v === 'object' ? JSON.stringify(v) : String(v),
        ])
      );
      const r = await promisify(execFile)(
        process.execPath,
        ['--input-type=module', '-e', RUNNER, 'network-conditions'],
        {
          encoding: 'utf8',
          env: { ...process.env, PORT: String(port), OUT: outputDir, ...env },
        }
      ).catch(error => error);
      const stdout = `${r.stdout ?? ''}${r.stderr ?? ''}`;
      const file = /\[ARTIFACT\] NETWORK_CONDITIONS (\S+)/.exec(stdout)?.[1];
      return {
        stdout,
        code: r.code ?? 0,
        report: file ? JSON.parse(readFileSync(file, 'utf8')) : undefined,
      };
    };
    const outcome = (report, suffix) =>
      report.requests.find(r => r.url.endsWith(suffix));
    const waitFor = async predicate => {
      for (let i = 0; i < 40; i++) {
        const value = await predicate();
        if (value) return value;
        await new Promise(r => setTimeout(r, 100));
      }
      throw new Error('condition not met in time');
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );

      // Block + mock + extra header in one load.
      const run = await conditions({
        NETCOND_URL: `${origin}/`,
        NETCOND_BLOCK_URLS: ['*/blocked/*'],
        NETCOND_MOCKS: [
          {
            match: '*/api/data',
            status: 200,
            headers: { 'content-type': 'text/plain' },
            body: MOCK,
          },
        ],
        NETCOND_EXTRA_HEADERS: { 'x-octo-test': 'present' },
      });
      assert.equal(run.code, 0, run.stdout);
      assert.equal(outcome(run.report, '/blocked/lib.js').outcome, 'blocked');
      const mocked = outcome(run.report, '/api/data');
      assert.equal(mocked.outcome, 'mocked');
      assert.equal(mocked.status, 200);
      assert.equal(run.report.page.loadEventReached, true);
      assert.match(run.stdout, /CONDITIONS_END_WITH_SESSION/);
      const requests = await waitFor(async () => {
        const all = await seen();
        return all.some(r => r.url.startsWith('/report')) && all;
      });
      const report = requests.find(r => r.url.startsWith('/report'));
      assert.equal(new URL(report.url, origin).searchParams.get('data'), MOCK);
      assert.ok(
        !requests.some(r => r.url === '/api/data'),
        'mocked request reached the server'
      );
      assert.ok(
        !requests.some(r => r.url.startsWith('/blocked')),
        'blocked request reached the server'
      );
      assert.equal(
        requests.find(r => r.url === '/').headers['x-octo-test'],
        'present'
      );

      // Offline: the load fails and the report says so.
      const before = (await seen()).length;
      const offline = await conditions({
        NETCOND_URL: `${origin}/`,
        NETCOND_OFFLINE: '1',
      });
      assert.equal(offline.report.page.loadEventReached, false);
      assert.match(
        offline.report.page.navigationError,
        /INTERNET_DISCONNECTED/
      );
      assert.equal(
        (await seen()).length,
        before,
        'offline load reached the server'
      );

      // Throttling does not stop a load; the applied limits are reported.
      const slow = await conditions({
        NETCOND_URL: `${origin}/`,
        NETCOND_LATENCY_MS: '150',
        NETCOND_DOWN_KBPS: '8000',
      });
      assert.equal(slow.report.page.loadEventReached, true);
      assert.equal(slow.report.applied.latencyMs, 150);
      assert.equal(outcome(slow.report, '/api/data').outcome, 'ok');

      // Invalid mocks fail clearly before the page is touched.
      const count = (await seen()).length;
      for (const mocks of [
        [{ match: '*/x', status: 99 }],
        [{ match: '', status: 200 }],
        [{ match: '*/x', status: 600 }],
      ]) {
        const bad = await conditions({
          NETCOND_URL: `${origin}/`,
          NETCOND_MOCKS: mocks,
        });
        assert.notEqual(bad.code, 0);
        assert.match(bad.stdout, /NETCOND_INVALID/);
        assert.equal(bad.report, undefined);
      }
      assert.equal(
        (await seen()).length,
        count,
        'an invalid option touched the page'
      );
    } finally {
      try {
        await cli('cleanup', { port });
      } finally {
        web.kill();
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
