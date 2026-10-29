import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const cli = resolve(import.meta.dirname, '../../dist/engine/cli.mjs');

// Async: the fake Chrome server answers from this process's event loop.
function run(cwd, ...args) {
  return new Promise(done => {
    const child = spawn(
      process.execPath,
      [cli, 'session', '--action', ...args],
      { cwd }
    );
    let stdout = '',
      stderr = '';
    child.stdout.on('data', chunk => (stdout += chunk));
    child.stderr.on('data', chunk => (stderr += chunk));
    child.on('close', status =>
      done({
        status,
        json: status === 0 ? JSON.parse(stdout) : null,
        stderr,
      })
    );
  });
}

async function fakeChrome(t, body) {
  const server = createServer((request, response) => {
    if (request.url !== '/json/version') {
      response.writeHead(404).end();
      return;
    }
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(body));
  });
  await new Promise(done => server.listen(0, '127.0.0.1', done));
  t.after(() => server.close());
  return server.address().port;
}

function workspace(t) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-session-cli-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test('session start binds the browser, then list, show and close it', async t => {
  const ws = workspace(t);
  const port = await fakeChrome(t, {
    webSocketDebuggerUrl: `ws://127.0.0.1:1/devtools/browser/abc-123`,
  });
  const started = await run(ws, 'start', '--port', String(port));
  assert.equal(started.status, 0, started.stderr);
  const { session } = started.json;
  assert.match(session.id, /^s-[0-9a-f]{16}$/);
  assert.deepEqual(session.browser, {
    incarnation: 'abc-123',
    endpoint: `http://127.0.0.1:${port}`,
  });
  const listed = (await run(ws, 'list')).json.sessions;
  assert.deepEqual(
    listed.map(row => [row.id, row.lifecycle]),
    [[session.id, 'active']]
  );
  const shown = (await run(ws, 'show', '--id', session.id)).json;
  assert.equal(shown.session.id, session.id);
  assert.equal(shown.lease, null);
  const closed = (await run(ws, 'close', '--id', session.id)).json;
  assert.equal(closed.session.lifecycle, 'closed');
  assert.equal((await run(ws, 'show', '--id', session.id)).json.lease, null);
});

test('session refuses unreachable browsers, unknown ids and misplaced flags', async t => {
  const ws = workspace(t);
  const silent = await fakeChrome(t, { Browser: 'no socket' });
  for (const [args, message] of [
    [['start', '--port', String(silent)], /reported no browser id/],
    [['show'], /show requires --id/],
    [['list', '--id', 's-0123456789abcdef'], /list does not take --id/],
    [['show', '--id', 's-0123456789abcdef'], /No session with this id/],
    [['close', '--id', '../escape'], /invalid s identifier/],
    [['list', '--port', '9222'], /--port applies only to start/],
    [['resume'], /session needs one of/],
  ]) {
    const out = await run(ws, ...args);
    assert.equal(out.status, 2, args.join(' '));
    assert.match(out.stderr, message, args.join(' '));
  }
});
