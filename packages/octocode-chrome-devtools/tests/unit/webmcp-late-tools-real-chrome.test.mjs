// A page that registers its WebMCP tool only after a slow fetch, against a real isolated headless
// Chrome (temporary workspace, own profile, local server): webmcp-tools run right after the
// navigation still lists and invokes the tool, and a page without tools reports none.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { createServer as createNetServer } from 'node:net';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { promisify } from 'node:util';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

// The tool registers once /slow answers, well after the document's load event.
const SLOW_MS = 2000;
const latePage = `<!doctype html><title>late</title><script>
fetch('/slow').then(r => r.text()).then(() => document.modelContext.registerTool({
  name: 'late_tool',
  description: 'Registered after a slow fetch',
  inputSchema: { type: 'object', properties: {} },
  annotations: { readOnlyHint: true },
  execute: async () => ({ content: [{ type: 'text', text: 'late ok' }] }),
}));
</script>`;

const freePort = () =>
  new Promise((resolve, reject) => {
    const server = createNetServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
    server.on('error', reject);
  });

const pageServer = () =>
  new Promise(resolve => {
    const server = createServer((req, res) => {
      if (req.url === '/slow') {
        setTimeout(() => res.end('done'), SLOW_MS);
      } else {
        res.setHeader('content-type', 'text/html');
        res.end(
          req.url === '/late' ? latePage : '<!doctype html><title>none</title>'
        );
      }
    }).listen(0, '127.0.0.1', () => resolve(server));
  });

test(
  'webmcp-tools lists and invokes a tool registered after load, and reports a page with none',
  { skip, timeout: 120000 },
  async t => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-webmcp-late-'));
    const port = await freePort();
    const server = await pageServer();
    const origin = `http://127.0.0.1:${server.address().port}`;
    // Async, so this process keeps serving the page while the CLI runs.
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
    const goto = async url => {
      const out = await cli('run', {
        connection: { port },
        plan: { steps: [{ op: 'goto', url }] },
      });
      assert.equal(out.ok, true, JSON.stringify(out.flow?.failure));
    };
    const check = async options => {
      const out = await cli('check', {
        recipe: 'webmcp-tools',
        connection: { port },
        options,
      });
      assert.equal(out.ok, true, JSON.stringify(out));
      return readFileSync(out.logs.stdout, 'utf8');
    };
    try {
      const opened = await cli('open', {
        port,
        headless: true,
        url: 'about:blank',
      });
      assert.equal(opened.ok, true, JSON.stringify(opened));

      await goto(`${origin}/late`);
      const listed = await check({ WEBMCP_WAIT_MS: '5000' });
      if (listed.includes('WEBMCP_UNAVAILABLE')) {
        t.skip('this Chrome has no WebMCP domain');
        return;
      }
      assert.match(listed, /\[WEBMCP_TOOL\] name=late_tool risk=read-only /);

      const invoked = await check({
        WEBMCP_ACTION: 'invoke',
        WEBMCP_TOOL: 'late_tool',
        WEBMCP_INPUT: '{}',
      });
      assert.match(
        invoked,
        /\[WEBMCP_RESULT\] tool=late_tool status=Completed output=.*late ok/
      );

      await goto(`${origin}/none`);
      assert.match(await check({}), /\[FINDING\] WEBMCP_NO_TOOLS /);
    } finally {
      try {
        await cli('cleanup', { port });
      } finally {
        server.close();
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
