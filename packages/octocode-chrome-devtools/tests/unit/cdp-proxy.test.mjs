// The CDP proxy against a fake browser (always runs) and real Chrome (skipped when none is installed).
import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
} from 'node:fs';
import { createServer as createHttp, request } from 'node:http';
import { createServer, connect } from 'node:net';
import { networkInterfaces, tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { promisify } from 'node:util';
import { WebSocketServer, WebSocket as WsClient } from 'ws';

const run = promisify(execFile);
const engine = join(import.meta.dirname, '../../dist/engine/cdp-proxy.mjs');
const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const LOOPBACK = ['127.0.0.1', '::1'];
const SECRET_PARAM = 'cookie-value-that-must-never-be-logged';

const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });

/** A fake browser: /json/version plus a CDP socket that echoes the method and the session it saw. */
async function fakeBrowser() {
  const seen = [];
  const sockets = new Set();
  const http = createHttp();
  const wss = new WebSocketServer({ noServer: true });
  http.on('request', (req, res) => {
    res.setHeader('content-type', 'application/json');
    res.end(
      JSON.stringify({
        Browser: 'Fake/1',
        webSocketDebuggerUrl: `ws://127.0.0.1:${http.address().port}/devtools/browser/fake`,
      })
    );
  });
  http.on('upgrade', (req, socket, head) =>
    wss.handleUpgrade(req, socket, head, ws => wss.emit('connection', ws))
  );
  wss.on('connection', ws => {
    sockets.add(ws);
    ws.on('message', data => {
      const message = JSON.parse(String(data));
      seen.push(message);
      const result = { echoMethod: message.method, echoParams: message.params };
      if (message.method === 'Target.attachToTarget')
        result.sessionId = `S${message.params.targetId}`;
      const reply = () =>
        ws.send(
          JSON.stringify({
            id: message.id,
            result,
            ...(message.sessionId && { sessionId: message.sessionId }),
          })
        );
      if (message.method === 'Test.slow') setTimeout(reply, 50);
      else reply();
      if (message.method === 'Test.emit')
        for (const sessionId of [undefined, message.params.sessionId])
          ws.send(
            JSON.stringify({
              method: 'Test.event',
              params: { n: 1 },
              ...(sessionId && { sessionId }),
            })
          );
    });
  });
  await new Promise(resolve => http.listen(0, '127.0.0.1', resolve));
  return {
    port: http.address().port,
    seen,
    dropClients: () => sockets.forEach(ws => ws.terminate()),
    close: () => {
      sockets.forEach(ws => ws.terminate());
      http.closeAllConnections();
      http.close();
    },
  };
}

const proxyCli = async (cwd, ...args) => {
  const result = await run(process.execPath, [engine, ...args], {
    cwd,
    encoding: 'utf8',
  }).catch(error => error);
  return { ...result, json: result.stdout ? JSON.parse(result.stdout) : null };
};

const connectClient = (url, options) =>
  new Promise((resolve, reject) => {
    const ws = new WsClient(url, options);
    const inbox = [];
    const waiting = [];
    ws.on('message', data => {
      const message = JSON.parse(String(data));
      const waiter = waiting.shift();
      if (waiter) waiter(message);
      else inbox.push(message);
    });
    ws.once('open', () =>
      resolve({
        ws,
        next: () =>
          new Promise((res, rej) => {
            if (inbox.length) return res(inbox.shift());
            const timer = setTimeout(() => rej(new Error('no message')), 5000);
            waiting.push(message => {
              clearTimeout(timer);
              res(message);
            });
          }),
        quiet: async ms => {
          await new Promise(r => setTimeout(r, ms));
          return inbox.length === 0;
        },
        call: async message => {
          ws.send(JSON.stringify(message));
          const reply = await new Promise((res, rej) => {
            const timer = setTimeout(() => rej(new Error('no reply')), 5000);
            const take = () => {
              const index = inbox.findIndex(m => m.id === message.id);
              if (index >= 0) {
                clearTimeout(timer);
                return res(inbox.splice(index, 1)[0]);
              }
              waiting.push(m => {
                inbox.push(m);
                take();
              });
            };
            take();
          });
          return reply;
        },
        close: () => ws.close(),
      })
    );
    ws.once('error', reject);
    ws.once('unexpected-response', (_req, res) => {
      reject(
        Object.assign(new Error(`HTTP ${res.statusCode}`), {
          status: res.statusCode,
        })
      );
    });
  });

const upgradeStatus = (port, path, headers = {}) =>
  new Promise((resolve, reject) => {
    const req = request({
      host: '127.0.0.1',
      port,
      path,
      headers: {
        Connection: 'Upgrade',
        Upgrade: 'websocket',
        'Sec-WebSocket-Version': '13',
        'Sec-WebSocket-Key': Buffer.from('0123456789abcdef').toString('base64'),
        ...headers,
      },
    });
    req.on('response', res => {
      res.resume();
      resolve(res.statusCode);
    });
    req.on('upgrade', (res, socket) => {
      socket.destroy();
      resolve(res.statusCode);
    });
    req.on('error', reject);
    req.end();
  });

let cwd, browser, started, urlText, token;
before(async () => {
  cwd = mkdtempSync(join(tmpdir(), 'octo-proxy-'));
  browser = await fakeBrowser();
  const result = await proxyCli(
    cwd,
    'start',
    '--port',
    String(browser.port),
    '--log'
  );
  assert.equal(
    result.json.status,
    'PROXY_STARTED',
    result.stdout + result.stderr
  );
  started = result.json;
  urlText = result.stdout;
  token = new URL(started.url).pathname.slice(1);
});
after(async () => {
  await proxyCli(cwd, 'stop', '--port', String(browser.port));
  browser.close();
  rmSync(cwd, { recursive: true, force: true });
});

test('start prints a labelled secret URL once; status never contains the token; the state file is 0600', async () => {
  assert.match(started.url, /^ws:\/\/127\.0\.0\.1:\d+\/[0-9a-f]{32}$/);
  assert.match(started.secret, /SECRET/);
  const status = await proxyCli(cwd, 'status', '--port', String(browser.port));
  assert.equal(status.json.status, 'PROXY_RUNNING');
  assert.equal(status.json.pid, started.pid);
  assert.equal(status.json.listenPort, started.listenPort);
  assert.ok(!status.stdout.includes(token));
  const stateFile = join(
    cwd,
    '.octocode/octocode-chrome-devtools/tmp/browser-state',
    `cdp-proxy-${browser.port}.json`
  );
  assert.equal(statSync(stateFile).mode & 0o777, 0o600);
  assert.ok(readFileSync(stateFile, 'utf8').includes(token));
  assert.equal(urlText.split(token).length - 1, 1);
});

test('only loopback addresses are bound', async () => {
  assert.ok(started.addresses.length >= 1);
  for (const address of started.addresses)
    assert.ok(LOOPBACK.includes(address));
  const external = Object.values(networkInterfaces())
    .flat()
    .find(entry => entry && entry.family === 'IPv4' && !entry.internal);
  if (!external) return;
  const refused = await new Promise(resolve => {
    const socket = connect(started.listenPort, external.address);
    socket.once('connect', () => (socket.destroy(), resolve(false)));
    socket.once('error', () => resolve(true));
  });
  assert.equal(refused, true);
});

test('a wrong token, a plain request and a foreign Origin are refused with 403 before the upgrade', async () => {
  const port = started.listenPort;
  assert.equal(await upgradeStatus(port, `/${'0'.repeat(32)}`), 403);
  assert.equal(await upgradeStatus(port, '/'), 403);
  assert.equal(
    await upgradeStatus(port, `/${token}`, { Origin: 'https://evil.example' }),
    403
  );
  assert.equal(
    await upgradeStatus(port, `/${token}`, { Host: 'rebind.example' }),
    403
  );
  assert.equal(
    await upgradeStatus(port, `/${token}`, { Origin: 'http://localhost:3000' }),
    101
  );
  await assert.rejects(connectClient(`ws://127.0.0.1:${port}/nope`), {
    status: 403,
  });
});

test('two clients with overlapping message ids each get their own responses, and ids reach the browser unique', async () => {
  const a = await connectClient(started.url);
  const b = await connectClient(started.url);
  const seenBefore = browser.seen.length;
  const [ra, rb] = await Promise.all([
    a.call({ id: 1, method: 'Test.slow', params: { who: 'a' } }),
    b.call({ id: 1, method: 'Test.fast', params: { who: 'b' } }),
  ]);
  assert.equal(ra.id, 1);
  assert.equal(rb.id, 1);
  assert.equal(ra.result.echoParams.who, 'a');
  assert.equal(rb.result.echoParams.who, 'b');
  const ids = browser.seen.slice(seenBefore).map(m => m.id);
  assert.equal(new Set(ids).size, ids.length);
  a.close();
  b.close();
});

test('browser events broadcast; session events reach only the clients using that session', async () => {
  const a = await connectClient(started.url);
  const b = await connectClient(started.url);
  const attached = await a.call({
    id: 7,
    method: 'Target.attachToTarget',
    params: { targetId: 'T' },
  });
  const sessionId = attached.result.sessionId;
  await a.call({ id: 8, method: 'Test.emit', params: { sessionId } });
  const eventsA = [await a.next(), await a.next()];
  assert.ok(
    eventsA.some(m => m.method === 'Test.event' && m.sessionId === sessionId)
  );
  assert.ok(
    eventsA.some(m => m.method === 'Test.event' && m.sessionId === undefined)
  );
  const eventB = await b.next();
  assert.equal(eventB.method, 'Test.event');
  assert.equal(eventB.sessionId, undefined);
  assert.equal(await b.quiet(150), true);
  // The session id is forwarded unchanged on commands.
  const reply = await a.call({ id: 9, method: 'Page.enable', sessionId });
  assert.equal(reply.sessionId, sessionId);
  assert.equal(browser.seen.at(-1).sessionId, sessionId);
  a.close();
  b.close();
});

test('a disconnecting client leaves the browser connection and other clients working', async () => {
  const a = await connectClient(started.url);
  const b = await connectClient(started.url);
  a.close();
  await new Promise(r => setTimeout(r, 100));
  const reply = await b.call({ id: 1, method: 'Browser.getVersion' });
  assert.equal(reply.result.echoMethod, 'Browser.getVersion');
  b.close();
});

test('--log appends one line per call with only the allowed fields: no params, no results', async () => {
  const client = await connectClient(started.url);
  await client.call({
    id: 1,
    method: 'Network.setCookie',
    params: { value: SECRET_PARAM },
  });
  client.close();
  // The daemon writes the line after the response is relayed: wait for it, not for a fixed delay.
  for (
    let tries = 0;
    tries < 100 &&
    !(
      existsSync(started.logFile) &&
      readFileSync(started.logFile, 'utf8').includes('Network.setCookie')
    );
    tries++
  )
    await new Promise(r => setTimeout(r, 50));
  assert.equal(statSync(started.logFile).mode & 0o777, 0o600);
  const raw = readFileSync(started.logFile, 'utf8');
  assert.ok(!raw.includes(SECRET_PARAM));
  const lines = raw
    .trim()
    .split('\n')
    .map(line => JSON.parse(line));
  assert.ok(lines.length > 0);
  for (const line of lines) {
    assert.deepEqual(
      Object.keys(line)
        .filter(key => line[key] !== undefined)
        .sort(),
      Object.keys(line)
        .filter(key =>
          ['ts', 'client', 'method', 'ok', 'durationMs', 'sessionId'].includes(
            key
          )
        )
        .sort()
    );
    assert.equal(typeof line.method, 'string');
    assert.equal(typeof line.ok, 'boolean');
    assert.equal(typeof line.durationMs, 'number');
  }
  assert.ok(lines.some(line => line.method === 'Network.setCookie'));
  // `status` reads the daemon's state file, which trails the live counters: poll, do not sleep.
  let status;
  for (let tries = 0; tries < 100; tries++) {
    status = await proxyCli(cwd, 'status', '--port', String(browser.port));
    if (status.json.messages >= lines.length) break;
    await new Promise(r => setTimeout(r, 50));
  }
  assert.equal(status.json.logFile, started.logFile);
  assert.ok(status.json.messages >= lines.length);
});

test('a second start for the same browser, a missing browser and a busy listen port fail clearly', async () => {
  const again = await proxyCli(cwd, 'start', '--port', String(browser.port));
  assert.equal(again.json.status, 'ERROR');
  assert.match(again.json.message, /already runs/);
  const missing = await proxyCli(
    cwd,
    'start',
    '--port',
    String(await freePort())
  );
  assert.match(missing.json.message, /No browser answers/);
  const busy = createServer();
  await new Promise(r => busy.listen(0, '127.0.0.1', r));
  const other = await fakeBrowser();
  try {
    const clash = await proxyCli(
      cwd,
      'start',
      '--port',
      String(other.port),
      '--listen-port',
      String(busy.address().port)
    );
    assert.equal(clash.json.status, 'ERROR');
    assert.match(clash.json.message, /already in use/);
  } finally {
    busy.close();
    other.close();
  }
});

test('stop frees the port and removes the state; the proxy exits when the browser connection drops', async () => {
  const other = await fakeBrowser();
  const one = await proxyCli(cwd, 'start', '--port', String(other.port));
  const listenPort = one.json.listenPort;
  const stopped = await proxyCli(cwd, 'stop', '--port', String(other.port));
  assert.equal(stopped.json.status, 'PROXY_STOPPED');
  await assert.rejects(upgradeStatus(listenPort, '/x'), {
    code: 'ECONNREFUSED',
  });
  const gone = await proxyCli(cwd, 'status', '--port', String(other.port));
  assert.equal(gone.json.status, 'PROXY_NOT_RUNNING');

  const two = await proxyCli(cwd, 'start', '--port', String(other.port));
  other.dropClients();
  for (
    let i = 0;
    i < 40 &&
    (() => {
      try {
        process.kill(two.json.pid, 0);
        return true;
      } catch {
        return false;
      }
    })();
    i++
  )
    await new Promise(r => setTimeout(r, 100));
  assert.throws(() => process.kill(two.json.pid, 0));
  other.close();
});

const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

test(
  'Browser.getVersion and Target.getTargets work through the proxy on real Chrome; cleanup stops the proxy',
  { skip, timeout: 120000 },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), 'octo-proxy-real-'));
    const port = await freePort();
    const raw = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd: dir,
        encoding: 'utf8',
      });
    try {
      assert.equal(raw('open', '--port', String(port), '--headless').status, 0);
      const result = await proxyCli(dir, 'start', '--port', String(port));
      assert.equal(
        result.json.status,
        'PROXY_STARTED',
        result.stdout + result.stderr
      );
      const client = await connectClient(result.json.url);
      const version = await client.call({
        id: 1,
        method: 'Browser.getVersion',
      });
      assert.match(version.result.product, /Chrome/);
      const targets = await client.call({ id: 1, method: 'Target.getTargets' });
      assert.ok(Array.isArray(targets.result.targetInfos));
      client.close();
      const status = await proxyCli(dir, 'status', '--port', String(port));
      assert.equal(status.json.status, 'PROXY_RUNNING');
      assert.ok(
        !status.stdout.includes(new URL(result.json.url).pathname.slice(1))
      );
    } finally {
      await proxyCli(dir, 'stop', '--port', String(port));
      raw('cleanup', '--port', String(port));
      rmSync(dir, { recursive: true, force: true });
    }
  }
);
