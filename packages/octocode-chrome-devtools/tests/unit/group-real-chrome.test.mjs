// Private tab groups on a real isolated headless Chrome (temporary workspace, loopback page server in a child process).
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
} from 'node:fs';
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
  res.setHeader('content-type', 'text/html');
  res.end('<!doctype html><title>T' + req.url + '</title><h1>PAGE ' + req.url + '</h1>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'private groups isolate cookies, list reconciled tabs, add, close and clean up on real Chrome',
  { skip, timeout: 270000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-group-')));
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
    const last = result => {
      assert.equal(result.status, 0, result.stdout + result.stderr);
      return JSON.parse(result.stdout.trim().split('\n').pop());
    };
    const group = (...args) => engine('group', ...args, '--port', port);
    const evaluate = (targetId, expression) => {
      const run = engine(
        'cdp',
        'Runtime.evaluate',
        '--port',
        port,
        '--target',
        targetId,
        '--acknowledge',
        '--params',
        JSON.stringify({ expression, returnByValue: true })
      );
      assert.equal(run.status, 0, run.stdout + run.stderr);
      // The page value lives in the cdp-1.json artifact the run reports.
      const artifact = /\[ARTIFACT\] cdp-1\.json (\S+)/.exec(
        run.stdout + run.stderr
      );
      assert.ok(artifact, run.stdout + run.stderr);
      return readFileSync(artifact[1], 'utf8');
    };
    const cookieOf = targetId => evaluate(targetId, 'document.cookie');
    const browserCall = async (method, params = {}) => {
      const version = await (
        await fetch(`http://127.0.0.1:${port}/json/version`)
      ).json();
      const ws = new WebSocket(version.webSocketDebuggerUrl);
      await new Promise(resolve => ws.addEventListener('open', resolve));
      const answer = new Promise(resolve =>
        ws.addEventListener('message', event => resolve(JSON.parse(event.data)))
      );
      ws.send(JSON.stringify({ id: 1, method, params }));
      const { result } = await answer;
      ws.close();
      return result;
    };
    const contexts = async () =>
      (await browserCall('Target.getBrowserContexts')).browserContextIds;
    const waitGone = async targetId => {
      for (let tries = 0; tries < 100; tries++) {
        const rows = await (
          await fetch(`http://127.0.0.1:${port}/json/list`)
        ).json();
        if (!rows.some(row => row.id === targetId)) return;
        await new Promise(r => setTimeout(r, 50));
      }
      assert.fail(`${targetId} is still listed`);
    };
    try {
      const launched = last(engine('open', '--port', port, '--headless'));
      assert.equal(launched.status, 'BROWSER_READY');
      const baseline = (await contexts()).length;

      // A plain tab sets a cookie; a private group's tab does not see it (and the reverse).
      const plain = last(
        engine('open', '--port', port, '--url', `${base}/plain`)
      );
      evaluate(plain.targetId, "document.cookie='plain=1;path=/'");
      const alpha = last(
        group('open', '--name', 'alpha', '--urls', `${base}/a0,${base}/a1`)
      );
      assert.equal(alpha.group, 'alpha');
      assert.equal(alpha.private, true);
      assert.deepEqual(
        alpha.tabs.map(tab => tab.status),
        ['loaded', 'loaded']
      );
      assert.equal(new Set(alpha.tabs.map(tab => tab.targetId)).size, 2);
      assert.doesNotMatch(cookieOf(alpha.tabs[0].targetId), /plain=1/);
      evaluate(alpha.tabs[0].targetId, "document.cookie='alpha=1;path=/'");
      assert.match(cookieOf(alpha.tabs[1].targetId), /alpha=1/);
      assert.doesNotMatch(cookieOf(plain.targetId), /alpha=1/);
      assert.equal((await contexts()).length, baseline + 1);

      // Two private groups are isolated from each other.
      const beta = last(group('open', '--urls', `${base}/b0`));
      assert.match(beta.group, /^g-[0-9a-f]{6}$/);
      assert.doesNotMatch(cookieOf(beta.tabs[0].targetId), /alpha=1/);
      assert.equal((await contexts()).length, baseline + 2);

      // A name that exists, odd names and urls outside the list create nothing.
      const duplicate = group('open', '--name', 'alpha');
      assert.notEqual(duplicate.status, 0);
      assert.match(duplicate.stdout + duplicate.stderr, /already exists/);
      for (const bad of ['has space', '../x', 'a/b', '', '-lead'])
        assert.notEqual(
          group('open', '--name', bad).status,
          0,
          `name ${JSON.stringify(bad)} must be refused`
        );
      assert.equal((await contexts()).length, baseline + 2);

      // add extends the group in the same context.
      const added = last(
        group('add', '--name', 'alpha', '--urls', `${base}/a2`)
      );
      assert.equal(added.private, true);
      assert.equal(added.tabs[0].status, 'loaded');
      assert.match(cookieOf(added.tabs[0].targetId), /alpha=1/);
      assert.equal((await contexts()).length, baseline + 2);
      const missing = group('add', '--name', 'nope', '--urls', `${base}/x`);
      assert.notEqual(missing.status, 0);
      assert.match(
        missing.stdout + missing.stderr,
        /Unknown group "nope"[\s\S]*alpha/
      );

      // list: names, privacy, counts, origin+path only, alive flags; selection and targets marks.
      const listed = last(group('list'));
      const alphaRow = listed.groups.find(row => row.name === 'alpha');
      assert.equal(alphaRow.private, true);
      assert.equal(alphaRow.tabCount, 3);
      assert.ok(alphaRow.tabs.every(tab => tab.alive));
      assert.equal(alphaRow.tabs[2].url, `${base}/a2`);
      assert.equal(alphaRow.tabs[2].title, 'T/a2');
      assert.ok(alphaRow.tabs.every(tab => !tab.url.includes('?')));
      const snapshot = engine(
        'snapshot',
        '--port',
        port,
        '--group',
        'alpha',
        '--group-index',
        '1'
      );
      assert.match(snapshot.stdout + snapshot.stderr, /PAGE \/a1/);
      const targets = JSON.parse(engine('targets', '--port', port).stdout);
      const marked = targets.find(row => row.id === alpha.tabs[1].targetId);
      assert.equal(marked.group, 'alpha');
      assert.equal(marked.groupIndex, 1);
      assert.equal(marked.private, true);
      assert.equal(
        targets.find(row => row.id === plain.targetId).group,
        undefined
      );

      // A tab the user closed shows alive:false once, then is pruned.
      await fetch(
        `http://127.0.0.1:${port}/json/close/${alpha.tabs[1].targetId}`
      );
      await waitGone(alpha.tabs[1].targetId);
      const afterUser = last(group('list')).groups.find(
        row => row.name === 'alpha'
      );
      assert.deepEqual(
        afterUser.tabs.map(tab => tab.alive),
        [true, false, true]
      );
      const pruned = last(group('list')).groups.find(
        row => row.name === 'alpha'
      );
      assert.equal(pruned.tabCount, 2);
      assert.ok(pruned.tabs.every(tab => tab.alive));

      // close --tab keeps the group; an id outside the group is refused.
      const outsider = group(
        'close',
        '--name',
        'alpha',
        '--tab',
        plain.targetId
      );
      assert.notEqual(outsider.status, 0);
      assert.equal(
        last(group('close', '--name', 'alpha', '--tab', alpha.tabs[0].targetId))
          .closedTab,
        alpha.tabs[0].targetId
      );
      const remaining = last(group('list')).groups.find(
        row => row.name === 'alpha'
      );
      assert.equal(remaining.tabCount, 1);
      assert.equal(remaining.tabs[0].targetId, added.tabs[0].targetId);
      assert.equal((await contexts()).length, baseline + 2);

      // close group disposes the context, never touches plain tabs and frees the name.
      const closed = last(group('close', '--name', 'alpha'));
      assert.equal(closed.closed, true);
      assert.equal((await contexts()).length, baseline + 1);
      assert.ok(
        (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).some(
          row => row.id === plain.targetId
        )
      );
      assert.ok(!last(group('list')).groups.some(row => row.name === 'alpha'));
      assert.equal(last(group('open', '--name', 'alpha')).private, true);
      last(group('close', '--name', 'alpha'));

      // A non-private group shares cookies with the default context and creates no context.
      const shared = last(
        group(
          'open',
          '--name',
          'shared',
          '--no-private',
          '--urls',
          `${base}/s0`
        )
      );
      assert.equal(shared.private, false);
      assert.match(cookieOf(shared.tabs[0].targetId), /plain=1/);
      assert.equal((await contexts()).length, baseline + 1);
      assert.equal(
        last(group('list')).groups.find(row => row.name === 'shared').private,
        false
      );
      last(group('close', '--name', 'shared'));
      assert.ok(
        (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).some(
          row => row.id === plain.targetId
        )
      );

      // allowOrigins: a url outside the list is refused before anything is created.
      last(engine('open', '--port', port, '--allow-origin', base));
      const before = (await contexts()).length;
      const refused = group(
        'open',
        '--name',
        'guarded',
        '--urls',
        `${base}/ok,http://localhost:1/`
      );
      assert.notEqual(refused.status, 0);
      assert.match(refused.stdout + refused.stderr, /not allowed/);
      assert.equal((await contexts()).length, before);
      assert.ok(
        !last(group('list')).groups.some(row => row.name === 'guarded')
      );
      const refusedAdd = group(
        'add',
        '--name',
        beta.group,
        '--urls',
        'http://localhost:1/'
      );
      assert.notEqual(refusedAdd.status, 0);
      assert.match(refusedAdd.stdout + refusedAdd.stderr, /not allowed/);

      // cleanup disposes every remaining private context.
      const cleanup = last(engine('cleanup', '--port', port));
      assert.equal(cleanup.status, 'CLEANED_UP');
      assert.ok(cleanup.disposedContexts >= 1);
    } finally {
      engine('cleanup', '--port', port);
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
