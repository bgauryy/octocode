// Tab groups, isolated contexts, attach and the redirect guard against a real isolated headless
// Chrome (temporary workspace, loopback page server in a child process).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';

const pkg = join(import.meta.dirname, '../..');
const bin = join(pkg, 'bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(String(port)));
    });
    server.on('error', reject);
  });

const serverSource = `
require('node:http').createServer((req, res) => {
  const host = req.headers.host.split(':')[1];
  if (req.url === '/redirect') { res.writeHead(302, { location: 'http://localhost:' + host + '/final' }); return res.end(); }
  res.setHeader('content-type', 'text/html');
  res.end('<!doctype html><title>' + req.url + '</title><h1>PAGE ' + req.url + '</h1>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'groups, isolation, cleanup disposal, attach and the redirect guard work on real Chrome',
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
    const cdp = async path =>
      (await fetch(`http://127.0.0.1:${port}${path}`)).json();
    const cookieOf = (group, index) => {
      const run = engine(
        'cdp',
        'Runtime.evaluate',
        '--port',
        port,
        '--group',
        group,
        '--group-index',
        String(index),
        '--params',
        JSON.stringify({ expression: 'document.cookie', returnByValue: true })
      );
      return run.stdout + run.stderr;
    };
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

      // A group of N urls: N distinct targets, selectable by group and index.
      const urls = [`${base}/zero`, `${base}/one`, `${base}/two`];
      const grouped = last(
        engine('open', '--port', port, '--urls', urls.join(','))
      );
      assert.equal(grouped.tabs.length, urls.length);
      assert.equal(
        new Set(grouped.tabs.map(tab => tab.targetId)).size,
        urls.length
      );
      assert.deepEqual(
        grouped.tabs.map(tab => tab.status),
        urls.map(() => 'loaded')
      );
      assert.match(grouped.group, /^g-/);
      const snapshot = engine(
        'snapshot',
        '--port',
        port,
        '--group',
        grouped.group,
        '--group-index',
        '1'
      );
      assert.match(snapshot.stdout + snapshot.stderr, /PAGE \/one/);
      const byId = engine(
        'snapshot',
        '--port',
        port,
        '--target',
        grouped.tabs[1].targetId
      );
      assert.match(byId.stdout + byId.stderr, /PAGE \/one/);
      const listed = engine('targets', '--port', port);
      assert.match(listed.stdout, new RegExp(`"group": "${grouped.group}"`));
      const badIndex = engine(
        'snapshot',
        '--port',
        port,
        '--group',
        grouped.group,
        '--group-index',
        '9'
      );
      assert.notEqual(badIndex.status, 0);
      assert.match(
        badIndex.stdout + badIndex.stderr,
        /out of range[\s\S]*\/two/
      );
      const unknown = engine('snapshot', '--port', port, '--group', 'nope');
      assert.match(unknown.stdout + unknown.stderr, /Unknown group "nope"/);
      await fetch(
        `http://127.0.0.1:${port}/json/close/${grouped.tabs[2].targetId}`
      );
      const closed = engine(
        'snapshot',
        '--port',
        port,
        '--group',
        grouped.group,
        '--group-index',
        '2'
      );
      assert.match(closed.stdout + closed.stderr, /is closed/);

      // Isolated group: cookies set in a plain tab do not reach it, and cleanup disposes the context.
      const plain = last(
        engine('open', '--port', port, '--url', `${base}/plain`)
      );
      const setPlain = engine(
        'cdp',
        'Runtime.evaluate',
        '--port',
        port,
        '--target',
        plain.targetId,
        '--params',
        JSON.stringify({ expression: "document.cookie='plain=1;path=/'" })
      );
      assert.equal(setPlain.status, 0, setPlain.stdout + setPlain.stderr);
      const isolated = last(
        engine(
          'open',
          '--port',
          port,
          '--urls',
          `${base}/i0,${base}/i1`,
          '--isolated'
        )
      );
      assert.equal(isolated.isolatedContext, true);
      assert.equal(isolated.tabs.length, 2);
      assert.doesNotMatch(cookieOf(isolated.group, 0), /plain=1/);
      const contexts = async () => {
        const browser = (await cdp('/json/version')).webSocketDebuggerUrl;
        const ws = new WebSocket(browser);
        await new Promise(resolve => ws.addEventListener('open', resolve));
        const answer = new Promise(resolve =>
          ws.addEventListener('message', event =>
            resolve(JSON.parse(event.data))
          )
        );
        ws.send(JSON.stringify({ id: 1, method: 'Target.getBrowserContexts' }));
        const { result } = await answer;
        ws.close();
        return result.browserContextIds;
      };
      assert.equal((await contexts()).length, 1);
      const targetsBefore = (await cdp('/json')).filter(t => t.type === 'page');
      assert.ok(targetsBefore.some(t => t.id === isolated.tabs[0].targetId));

      // Attach: states the facts, never launches.
      const attached = last(engine('open', '--port', port, '--attach'));
      assert.equal(attached.attached, true);
      assert.match(attached.browserVersion, /Chrome/);
      assert.ok(attached.openTabCount >= 3);
      assert.ok(attached.openTabs.every(url => !url.includes('?')));
      assert.match(attached.profile, /isolated/);

      const cleanup = last(engine('cleanup', '--port', port));
      assert.equal(cleanup.status, 'CLEANED_UP');
      assert.equal(cleanup.disposedContexts, 1);
    } finally {
      engine('cleanup', '--port', port);
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
