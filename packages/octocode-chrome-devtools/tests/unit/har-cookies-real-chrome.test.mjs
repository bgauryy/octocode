// Cookie evidence on a real isolated headless Chrome: a login POST answers 302 with Set-Cookie and
// the next request carries the cookie. The live HAR and the network-events jsonl keep the header
// and cookie NAMES, with values redacted; the cookie value is never written or printed.
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
if (process.argv[1] === 'har') {
  const { run } = await import(${JSON.stringify(join(dist, 'cdp-checks/live-har-monitor.mjs'))});
  await run(cdp, { url: process.env.PAGE, monitorMs: 6000, waitUntil: 'networkIdle', idleMs: 800 });
} else {
  const recipe = await import(${JSON.stringify(join(dist, 'cdp-checks/network-events-check.mjs'))});
  await recipe.run(cdp);
}
cdp.close();
process.exit(process.exitCode ?? 0);
`;

const COOKIE_NAME = 'sessid';
const COOKIE_VALUE = 'cookie-value-sekret-4471';
const page = `<!doctype html><title>login</title><script>
fetch('/login', { method: 'POST', body: 'user=a' }).then(() => fetch('/api'));
</script>`;
const serverSource = `
const http = require('node:http');
const server = http.createServer((req, res) => {
  if (req.url === '/login' && req.method === 'POST') {
    res.writeHead(302, { location: '/home', 'set-cookie': '${COOKIE_NAME}=${COOKIE_VALUE}; Path=/; HttpOnly' });
    res.end();
  } else if (req.url === '/api') {
    res.writeHead(200, { 'content-type': 'text/plain' });
    res.end('ok');
  } else if (req.url === '/home') {
    res.writeHead(200, { 'content-type': 'text/plain' });
    res.end('home');
  } else {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(${JSON.stringify(page)});
  }
});
server.listen(0, '127.0.0.1', () => console.log(server.address().port));
`;

test(
  'HAR and network-events keep cookie header and cookie names with redacted values',
  { skip, timeout: 150000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-har-cookies-'));
    const web = spawn(process.execPath, ['-e', serverSource], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
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
    const record = async (mode, env) => {
      const outputDir = mkdtempSync(join(cwd, 'out-'));
      const r = await promisify(execFile)(
        process.execPath,
        ['--input-type=module', '-e', RUNNER, mode],
        {
          encoding: 'utf8',
          env: {
            ...process.env,
            PORT: String(port),
            OUT: outputDir,
            PAGE: url,
            ...env,
          },
        }
      ).catch(error => error);
      return { outputDir, stdout: `${r.stdout ?? ''}${r.stderr ?? ''}` };
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );
      const har = await record('har');
      const harText = readFileSync(
        join(har.outputDir, 'live-network.har'),
        'utf8'
      );
      const entries = JSON.parse(harText).log.entries;
      const header = (message, name) =>
        message.headers.find(h => h.name.toLowerCase() === name);

      const login = entries.find(
        e => new URL(e.request.url).pathname === '/login'
      );
      assert.ok(login, 'login entry');
      assert.equal(login.response.status, 302);
      assert.match(
        header(login.response, 'set-cookie').value,
        /^\[redacted \d+ chars\]$/
      );
      assert.deepEqual(
        login['_cookies']
          .filter(c => c.direction === 'response')
          .map(c => [c.name, c.path]),
        [[COOKIE_NAME, '/']]
      );

      // HAR-standard cookies: names and attributes, the value redacted.
      assert.deepEqual(login.response.cookies, [
        {
          name: COOKIE_NAME,
          value: `[redacted ${COOKIE_VALUE.length} chars]`,
          path: '/',
          httpOnly: true,
        },
      ]);

      const api = entries.find(e => new URL(e.request.url).pathname === '/api');
      assert.ok(api, 'api entry');
      assert.match(
        header(api.request, 'cookie').value,
        /^\[redacted \d+ chars\]$/
      );
      assert.ok(
        api['_cookies'].some(
          c => c.direction === 'request' && c.name === COOKIE_NAME
        ),
        JSON.stringify(api['_cookies'])
      );
      assert.deepEqual(
        api.request.cookies.map(c => [c.name, c.value]),
        [[COOKIE_NAME, `[redacted ${COOKIE_VALUE.length} chars]`]]
      );
      assert.ok(
        harText.includes('Set-Cookie') ||
          harText.toLowerCase().includes('set-cookie')
      );
      assert.ok(harText.includes(COOKIE_NAME));
      for (const text of [harText, har.stdout])
        assert.ok(!text.includes(COOKIE_VALUE), 'cookie value leaked');

      // Cookie evidence is the same in the network-events jsonl.
      const events = await record('events', {
        NETEVENTS_URL: url,
        NETEVENTS_MS: '2500',
        NETEVENTS_KINDS: '["http"]',
      });
      const raw = readFileSync(
        join(events.outputDir, 'network-events.jsonl'),
        'utf8'
      );
      const rows = raw
        .trim()
        .split('\n')
        .map(line => JSON.parse(line));
      assert.ok(
        rows.some(
          r =>
            r.type === 'response-extra' &&
            r.cookies.some(c => c.name === COOKIE_NAME)
        ),
        'response-extra cookie name'
      );
      assert.ok(
        rows.some(
          r =>
            r.type === 'request-extra' &&
            r.cookies.some(c => c.name === COOKIE_NAME)
        ),
        'request-extra cookie name'
      );
      assert.match(raw, /"[Ss]et-[Cc]ookie":"\[redacted \d+ chars\]"/);
      for (const text of [raw, events.stdout])
        assert.ok(!text.includes(COOKIE_VALUE), 'cookie value leaked');
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
