// The nine capture recipes added with typed options, run through the real `check` path of the MCP
// server (typed options -> engine env -> Node-permission sandbox -> recipe) on real Chrome. Their
// own tests call the recipe modules directly; this one proves the options survive the sandbox's
// environment allowlist, which is where a new recipe's keys are silently dropped if it is stale.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
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

const sourceMap = Buffer.from(
  JSON.stringify({
    version: 3,
    file: 'app.js',
    sources: ['src/app.ts'],
    names: ['used'],
    mappings: 'AAAA',
  })
).toString('base64');
const appJs = `function used() { return 1; }\nfunction unused() { return 2; }\nused();\n//# sourceMappingURL=data:application/json;base64,${sourceMap}\n`;
const serverSource = `
const app = ${JSON.stringify(appJs)};
require('node:http').createServer((req, res) => {
  if (req.url === '/app.js') { res.setHeader('content-type', 'text/javascript'); return res.end(app); }
  res.setHeader('content-type', 'text/html');
  res.end('<!doctype html><title>recipes</title><style>.used{color:red}.unused{color:blue}</style><h1 class="used">Recipes</h1><script src="/app.js"></script><script>localStorage.setItem("k","v")</script>');
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'each new recipe runs through check with typed options and the sandbox forwards them',
  { skip, timeout: 240000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-recipes-')));
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
    // recipe, options, and a pattern the recipe's own stdout must contain only if the options arrived.
    /** @type {[string, Record<string, unknown>, RegExp][]} */
    const rows = [
      ['script-sources', { SCRIPTS_URL: `${base}/` }, /\[SCRIPT\] .*\/app\.js/],
      [
        'source-map',
        { SOURCEMAP_SCRIPT: `${base}/app.js`, SOURCEMAP_POSITIONS: '1:0' },
        /\[POSITION\] 1:0 -> src\/app\.ts/,
      ],
      ['storage-dump', { STORAGE_DUMP_URL: `${base}/` }, /localStorage=1/],
      [
        'network-events',
        { NETEVENTS_URL: `${base}/`, NETEVENTS_MS: 1200 },
        /\[METRIC\] NETWORK_EVENTS windowMs=1200/,
      ],
      [
        'network-conditions',
        { NETCOND_URL: `${base}/`, NETCOND_BLOCK_URLS: ['*app.js'] },
        /blocked=1/,
      ],
      [
        'coverage',
        { COVERAGE_URL: `${base}/`, COVERAGE_MS: 500 },
        /\[METRIC\] COVERAGE js .*unused=[1-9]/,
      ],
      [
        'trace-capture',
        { TRACE_URL: `${base}/`, TRACE_MS: 800 },
        /\[METRIC\] TRACE bytes=[1-9]/,
      ],
      [
        'cdp-domain',
        { CDPDOM_DOMAIN: 'Performance', CDPDOM_MODE: 'dump' },
        /\[METRIC\] CDP_DOMAIN domain=Performance methods=1 ok=1/,
      ],
      ['heap-snapshot', {}, /\[METRIC\] HEAP_SNAPSHOT bytes=[1-9]/],
      ['page-pdf', {}, /\[METRIC\] PAGE_PDF bytes=[1-9]/],
    ];
    try {
      const opened = await client.callTool({
        name: 'open',
        arguments: { port, headless: true, url: `${base}/` },
      });
      assert.ok(
        !opened.isError,
        JSON.stringify(opened.structuredContent).slice(0, 200)
      );
      for (const [recipe, options, pattern] of rows) {
        const reply = await client.callTool({
          name: 'check',
          arguments: {
            recipe,
            connection,
            ...(Object.keys(options).length ? { options } : {}),
          },
        });
        const data = reply.structuredContent ?? {};
        assert.ok(
          !reply.isError,
          `${recipe}: ${JSON.stringify(data).slice(0, 300)}`
        );
        const stdout =
          data.stdout ??
          // A reply with files names its capture manifest; the manifest names the stdout log.
          (data.evidence
            ? readFileSync(
                JSON.parse(readFileSync(join(cwd, data.evidence), 'utf8')).logs
                  .stdout,
                'utf8'
              )
            : '');
        assert.match(
          stdout,
          pattern,
          `${recipe} did not see its options (sandbox env allowlist?):\n${stdout.slice(0, 400)}`
        );
      }
    } finally {
      await client
        .callTool({ name: 'cleanup', arguments: { port } })
        .catch(() => undefined);
      await client.close();
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
