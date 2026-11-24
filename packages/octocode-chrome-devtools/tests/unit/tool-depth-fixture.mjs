// Shared harness for the tool-depth tests: a loopback fixture site in a child process, a temporary
// workspace, and an MCP client on the full preset. Callers get `call` (success or refusal text).
import { DIE_WITH_PARENT } from '../fixtures/die-with-parent.mjs';
import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
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
export const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

/** What the fixture page offers: its title, its controls (role, accessible name) and its cookie. */
export const page = {
  title: 'depth-fixture',
  controls: [
    ['h1', 'Heading'],
    ['button', 'Go'],
    ['textbox', 'Email'],
    ['link', 'Docs'],
  ],
  cookie: { name: 'sessname', value: 'SECRETVALUE123' },
};

const serverSource = `
const page = ${JSON.stringify(page)};
const html = '<!doctype html><title>' + page.title + '</title><link rel="stylesheet" href="/app.css"><h1>Heading</h1><button>Go</button><input aria-label="Email"><a href="/docs">Docs</a><script src="/app.js"></script>';
const types = { '/app.js': 'text/javascript', '/app.css': 'text/css' };
const bodies = {
  '/app.js': 'function used(){return 1} function neverCalled(){return "x".repeat(40)} used();',
  '/app.css': '.used{color:red} .neverMatched{color:blue;margin:1px}',
};
require('node:http').createServer((req, res) => {
  res.setHeader('content-type', types[req.url] ?? 'text/html');
  if (req.url === '/') res.setHeader('set-cookie', page.cookie.name + '=' + page.cookie.value + '; Path=/');
  res.end(bodies[req.url] ?? html);
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

export const withSite = async (body, { startBrowser = true } = {}) => {
  const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-depth-')));
  mkdirSync(join(cwd, '.octocode'), { recursive: true });
  const web = spawn(
    process.execPath,
    [...DIE_WITH_PARENT, '-e', serverSource],
    {
      stdio: ['pipe', 'pipe', 'inherit'],
    }
  );
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
    (reply.content ?? []).map(part => part.text ?? '').join('');
  /** Calls a tool; success returns the structured reply, a refusal returns `{ refused: text }`. */
  const call = async (name, args) => {
    const reply = await client.callTool({ name, arguments: args });
    if (reply.isError) return { refused: text(reply) };
    return { ...reply.structuredContent, text: text(reply) };
  };
  const ok = async (name, args) => {
    const reply = await call(name, args);
    if (reply.refused !== undefined)
      throw new Error(`${name} refused: ${reply.refused.slice(0, 400)}`);
    return reply;
  };
  const read = file => readFileSync(join(cwd, file));
  try {
    if (startBrowser)
      await ok('open', { port, headless: true, url: `${base}/` });
    await body({ cwd, base, port, connection, client, call, ok, read });
  } finally {
    await client
      .callTool({ name: 'cleanup', arguments: { port } })
      .catch(() => undefined);
    await client.close();
    web.kill();
    rmSync(cwd, { recursive: true, force: true });
  }
};
