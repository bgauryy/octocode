// Every tool of the full MCP surface is called once against a real isolated headless Chrome
// (temporary workspace, loopback fixture server in a child process). A tool the contract lists but
// the engine cannot answer is caught here, whatever its unit tests say. Each row states what a
// healthy answer is: the call succeeds, or the engine refuses with the named, intended reason.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';
import { connectStdio } from '../../dist/runtime.js';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const serverSource = `
require('node:http').createServer((req, res) => {
  res.setHeader('content-type', req.url === '/api' ? 'application/json' : 'text/html');
  res.end(req.url === '/api' ? '{"ok":true}' : '<!doctype html><title>smoke</title><h1>Smoke</h1><button>Go</button><script>localStorage.setItem("k","v")</script>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'every tool of the full surface answers once on real Chrome',
  { skip, timeout: 240000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-smoke-')));
    mkdirSync(join(cwd, '.octocode'), { recursive: true });
    const web = spawn(process.execPath, ['-e', serverSource], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    const base = `http://127.0.0.1:${await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(String(chunk).trim()))
    )}`;
    const port = await freePort();
    const connection = { port };
    const client = await connectStdio({
      command: process.execPath,
      args: [bin, '--mcp', '--preset', 'full'],
      cwd,
    });
    const text = reply =>
      reply.structuredContent
        ? JSON.stringify(reply.structuredContent)
        : (reply.content ?? []).map(part => part.text ?? '').join('');
    const results = new Map();
    const call = async (name, args, expectation = 'ok') => {
      const reply = await client.callTool({ name, arguments: args });
      const body = text(reply);
      results.set(name, { isError: !!reply.isError, body });
      if (expectation === 'ok')
        assert.equal(
          reply.isError ?? false,
          false,
          `${name}: ${body.slice(0, 300)}`
        );
      else
        assert.match(
          body,
          expectation,
          `${name} should be refused for the intended reason: ${body.slice(0, 300)}`
        );
      return reply.structuredContent ?? {};
    };
    try {
      const { tools } = await client.listTools();
      const surface = tools.map(tool => tool.name);

      await call('open', { port, headless: true, url: `${base}/` });
      await call('targets', { connection });
      await call('snapshot', { connection });
      await call('screenshot', { connection });
      await call('run', {
        connection,
        plan: { waitMs: 3000, steps: [{ op: 'snapshot' }] },
      });
      await call('cdp', { method: 'Browser.getVersion', connection });
      await call('protocol', { member: 'DOM.querySelector', connection });
      await call('schema', { command: 'run' });
      await call('skill', {});
      await call('flow', { action: 'list' });
      await call('check', {
        recipe: 'page-snapshot',
        connection,
      });
      await call('dev-check', { url: `${base}/`, port });
      writeFileSync(
        join(cwd, '.octocode/smoke-script.mjs'),
        "export async function run(cdp) { console.log('smoke ' + (await cdp.send('Browser.getVersion')).product); }\n"
      );
      await call('script', { file: '.octocode/smoke-script.mjs', connection });

      // Saved-evidence readers: use the capture the first call left behind.
      const snap = await call('snapshot', { connection });
      const capture = snap.capture?.file;
      assert.ok(capture, 'snapshot returns its capture manifest path');
      await call('artifact', { file: capture, format: 'json', length: 500 });
      await call('query', {
        file: capture,
        format: 'json',
        pointer: '/artifacts',
        limit: 2,
      });

      // Helpers that take only CLI arguments: their usage must be reachable through the MCP wiring.
      for (const name of [
        'snapshot-query',
        'measure-query',
        'har-pager',
        'har-redact',
        'api-replay',
        'prune',
        'protocol-corpus',
        'har-ingest',
        'corpus-query',
      ])
        await call(name, { args: ['--help'] });

      await call('session', { action: 'list' });
      await call('cookies', {
        from: { port },
        confirmDomains: ['127.0.0.1'],
        exportStorageState: '.octocode/smoke-state.json',
        acknowledgeSecrets: true,
      });
      // Sensitive and blocked raw methods are refused for the intended reason, not sent.
      await call('cdp', { method: 'Browser.close', connection }, /blocked/i);
      await call(
        'cdp',
        {
          method: 'Browser.grantPermissions',
          params: { permissions: ['geolocation'] },
          connection,
        },
        /permission|acknowledge/i
      );

      const started = await call('proxy', { action: 'start', port });
      assert.ok(
        JSON.stringify(started).includes('ws://127.0.0.1:'),
        'proxy start prints its loopback URL'
      );
      await call('proxy', { action: 'status', port });
      await call('proxy', { action: 'stop', port });

      await call('cleanup', { port });

      // Every listed tool was exercised exactly by name: a new tool without a row fails here.
      for (const name of surface)
        assert.ok(results.has(name), `no smoke call for tool ${name}`);
    } finally {
      await call('cleanup', { port }).catch(() => undefined);
      await client.close();
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
