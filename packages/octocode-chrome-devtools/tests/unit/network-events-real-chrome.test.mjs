// network-events on a real isolated headless Chrome: a WebSocket echo exchange (frames in both
// directions, in order), a Server-Sent Events stream, and credential headers redacted.
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

const TOKEN = 'Bearer sekret-token-81';
const SESSION = 'sekret-session-82';
const page = `<!doctype html><title>events</title><script>
const ws = new WebSocket('ws://' + location.host + '/socket');
ws.onopen = () => { ws.send('first'); ws.send('second'); };
ws.onmessage = e => { if (e.data === 'echo:second') ws.close(); };
new EventSource('/stream').onmessage = e => { if (e.data === 'tick-3') e.target.close(); };
fetch('/api', { headers: { Authorization: ${JSON.stringify(TOKEN)} } });
</script>`;

// ws is the fixture server library; the child process keeps this event loop free.
const serverSource = `
const { WebSocketServer } = require('ws');
const http = require('node:http');
const server = http.createServer((req, res) => {
  if (req.url === '/stream') {
    res.writeHead(200, { 'content-type': 'text/event-stream' });
    let i = 0;
    const timer = setInterval(() => {
      res.write('data: tick-' + ++i + '\\n\\n');
      if (i === 3) { clearInterval(timer); res.end(); }
    }, 100);
  } else if (req.url === '/api') {
    res.writeHead(200, { 'content-type': 'text/plain', 'set-cookie': 'sid=${SESSION}; Path=/' });
    res.end('ok');
  } else {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(${JSON.stringify(page)});
  }
});
const wss = new WebSocketServer({ server, path: '/socket' });
wss.on('connection', socket => socket.on('message', m => socket.send('echo:' + m)));
server.listen(0, '127.0.0.1', () => console.log(server.address().port));
`;

test(
  'network-events records a websocket echo exchange in order, an SSE stream, and redacts credential headers',
  { skip, timeout: 150000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-net-events-'));
    const web = spawn(process.execPath, ['-e', serverSource], {
      cwd: join(import.meta.dirname, '../..'),
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
    const record = async env => {
      const outputDir = mkdtempSync(join(cwd, 'out-'));
      const r = await promisify(execFile)(
        process.execPath,
        ['--input-type=module', '-e', RUNNER, 'network-events'],
        {
          encoding: 'utf8',
          env: { ...process.env, PORT: String(port), OUT: outputDir, ...env },
        }
      ).catch(error => error);
      const stdout = `${r.stdout ?? ''}${r.stderr ?? ''}`;
      const file = /\[ARTIFACT\] NETWORK_EVENTS (\S+)/.exec(stdout)?.[1];
      const events = file
        ? readFileSync(file, 'utf8')
            .trim()
            .split('\n')
            .map(line => JSON.parse(line))
        : [];
      return { stdout, file, events, outputDir };
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );
      const { stdout, file, events, outputDir } = await record({
        NETEVENTS_URL: url,
        NETEVENTS_MS: '2500',
      });
      assert.ok(file, stdout);

      // WebSocket: handshake, then every frame in both directions in the order they happened.
      const frames = events.filter(
        e => e.kind === 'websocket' && e.type.startsWith('frame-')
      );
      const payloads = frames.map(
        e =>
          `${e.direction}:${readFileSync(join(outputDir, e.payloadFile), 'utf8')}`
      );
      assert.deepEqual(payloads, [
        'sent:first',
        'sent:second',
        'received:echo:first',
        'received:echo:second',
      ]);
      assert.ok(
        frames.every((f, i) => i === 0 || f.n > frames[i - 1].n),
        'events are ordered'
      );
      assert.ok(
        events.some(e => e.type === 'created' && e.url.startsWith('ws://'))
      );
      assert.ok(
        events.some(e => e.type === 'handshakeResponse' && e.status === 101)
      );
      assert.ok(events.some(e => e.type === 'closed'));

      // SSE: each message with its payload sidecar.
      const sse = events.filter(e => e.kind === 'sse');
      assert.deepEqual(
        sse.map(e => readFileSync(join(outputDir, e.payloadFile), 'utf8')),
        ['tick-1', 'tick-2', 'tick-3']
      );

      // HTTP events exist; credential header values are redacted in the jsonl and stdout.
      assert.ok(events.some(e => e.kind === 'http' && e.type === 'response'));
      const raw = readFileSync(file, 'utf8');
      assert.match(raw, /"[Aa]uthorization":"\[redacted \d+ chars\]"/);
      for (const secret of [TOKEN, SESSION]) {
        assert.ok(!raw.includes(secret), 'credential in jsonl');
        assert.ok(!stdout.includes(secret), 'credential in stdout');
      }
      assert.match(
        stdout,
        /\[METRIC\] NETWORK_EVENTS .*websocket=\d+ sse=3 http=\d+/
      );
      assert.match(stdout, /\[NEXT\] .*evidence-query/);

      // Filters: kinds and url substring.
      const onlySse = await record({
        NETEVENTS_URL: url,
        NETEVENTS_MS: '1500',
        NETEVENTS_KINDS: '["sse"]',
      });
      assert.ok(
        onlySse.events.length > 0 && onlySse.events.every(e => e.kind === 'sse')
      );
      const matched = await record({
        NETEVENTS_URL: url,
        NETEVENTS_MS: '1500',
        NETEVENTS_MATCH: '/api',
      });
      assert.ok(
        matched.events.length > 0 &&
          matched.events.every(e => e.url.includes('/api'))
      );

      // Bad input fails before observing anything.
      const bad = await record({ NETEVENTS_MS: '0' });
      assert.match(bad.stdout, /NETWORK_EVENTS_INVALID/);
      assert.equal(bad.file, undefined);
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
