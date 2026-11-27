// The agent's whole session path against a real isolated headless Chrome (temporary workspace,
// own profiles, a loopback page server): inline snapshot refs, a login plan, a domain-confirmed
// cookie transfer into a second browser, and the sandbox refusing a non-loopback request.
import { spawnPageServer } from '../fixtures/page-server.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

// Text typed into the form must never be persisted by the engine's evidence files.
const TYPED = 'typed-secret-8c41f7';

const page = `<!doctype html><title>Fixture</title><h1>Login</h1>
<form onsubmit="document.cookie='sid=abc123;path=/';out.textContent='Welcome';return false">
<input aria-label=User><button>Sign in</button></form><p id=out></p>`;

test(
  'inline snapshot, login plan, confirmed cookie transfer and the sandbox network refusal work on real Chrome',
  { skip, timeout: 180000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-e2e-'));
    // The page server is a child process: spawnSync below blocks this process's event loop.
    const web = spawnPageServer(page);
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
    const [from, to] = [await freePort(), await freePort()];
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    try {
      assert.equal(cli('open', { port: from, headless: true, url }).ok, true);
      const snapshot = cli('snapshot', { connection: { port: from } });
      assert.match(snapshot.snapshot, /textbox "User"/);
      assert.match(snapshot.snapshot, /button "Sign in"/);

      const login = cli('run', {
        connection: { port: from },
        plan: {
          waitMs: 3000,
          steps: [
            {
              op: 'act',
              action: 'fill',
              role: 'textbox',
              name: 'User',
              value: TYPED,
            },
            {
              op: 'act',
              action: 'click',
              role: 'button',
              name: 'Sign in',
              after: { text: 'Welcome' },
            },
          ],
        },
      });
      assert.equal(login.ok, true);
      const leaked = [];
      const scan = dir => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const path = join(dir, entry.name);
          if (entry.isDirectory()) {
            // Chrome's own profile folders are the browser's, not the engine's evidence.
            if (!entry.name.startsWith('cdp-chrome-profile')) scan(path);
          } else if (readFileSync(path).includes(TYPED)) leaked.push(path);
        }
      };
      scan(join(cwd, '.octocode'));
      assert.deepEqual(leaked, [], 'typed text persisted in evidence files');

      // The transfer must name its domains: a missing list is refused, a wrong one moves nothing.
      const stateFile = '.octocode/state.json';
      const base = [
        'cookies',
        '--i-understand-secrets',
        '--from-port',
        String(from),
        '--export-storage-state',
        stateFile,
      ];
      const refused = engine(...base);
      assert.notEqual(refused.status, 0);
      assert.match(refused.stderr, /--confirm-domains/);
      const wrong = engine(...base, '--confirm-domains', 'other.invalid');
      assert.match(wrong.stdout, /cookies=0/);
      const moved = engine(...base, '--confirm-domains', '127.0.0.1');
      assert.equal(moved.status, 0, moved.stderr);
      assert.match(moved.stdout, /cookie-names=sid/);

      // The saved state reaches a fresh browser, read back through a runtime CDP script.
      assert.equal(cli('open', { port: to, headless: true }).ok, true);
      const injected = engine(
        'cookies',
        '--i-understand-secrets',
        '--from-storage-state',
        stateFile,
        '--to-port',
        String(to),
        '--urls',
        url
      );
      assert.equal(injected.status, 0, injected.stderr);
      writeFileSync(
        join(cwd, '.octocode/probe.mjs'),
        `export async function run(cdp) {
  const { cookies } = await cdp.send('Network.getCookies', { urls: [${JSON.stringify(url)}] });
  console.log('names=' + cookies.map(c => c.name).join(','));
}\n`
      );
      const probe = engine(
        'script',
        '.octocode/probe.mjs',
        '--port',
        String(to)
      );
      assert.match(probe.stdout + probe.stderr, /names=sid/);

      // A script cannot become a network client: a non-loopback request is refused by the guard.
      writeFileSync(
        join(cwd, '.octocode/net.mjs'),
        `import https from 'node:https';
export async function run() {
  await new Promise(done => https.get('https://example.com', () => done()).on('error', done));
}\n`
      );
      const blocked = engine(
        'script',
        '.octocode/net.mjs',
        '--port',
        String(to)
      );
      assert.match(blocked.stdout + blocked.stderr, /\[SANDBOX\] net blocked/);
    } finally {
      for (const port of [from, to]) cli('cleanup', { port });
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);

test(
  'one run call fills, clicks with a postcondition, skips an optional failure, then snapshots and screenshots',
  { skip, timeout: 180000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-plan-'));
    const web = spawnPageServer(page);
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
    const port = await freePort();
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    try {
      assert.equal(cli('open', { port, headless: true, url }).ok, true);
      const result = cli('run', {
        connection: { port },
        plan: {
          waitMs: 3000,
          steps: [
            {
              op: 'act',
              action: 'fill',
              role: 'textbox',
              name: 'User',
              value: 'bob',
            },
            {
              op: 'act',
              action: 'click',
              role: 'button',
              name: 'No such button',
              optional: true,
              timeoutMs: 1500,
            },
            {
              op: 'act',
              action: 'click',
              role: 'button',
              name: 'Sign in',
              after: { text: 'Welcome' },
            },
            { op: 'snapshot' },
            { op: 'screenshot' },
          ],
        },
      });
      assert.equal(result.ok, true, JSON.stringify(result));
      assert.equal(result.flow.requestedSteps, 5);
      assert.equal(result.flow.completedSteps, 4);
      assert.deepEqual(
        result.flow.failedOptionalSteps.map(entry => entry.step),
        [2]
      );
      const rows = Object.fromEntries(
        result.flow.steps.map(row => [row.op + row.index, row])
      );
      assert.equal(rows.act2.status, 'failed');
      assert.equal(rows.act2.optional, true);
      for (const key of ['snapshot4', 'screenshot5']) {
        assert.equal(rows[key].status, 'complete', key);
        assert.ok(rows[key].next, `${key} keeps a read continuation`);
      }
    } finally {
      cli('cleanup', { port });
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
