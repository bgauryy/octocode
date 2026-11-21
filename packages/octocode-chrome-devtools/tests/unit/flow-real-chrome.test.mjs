// `flow run login-form` on a real isolated headless Chrome behaves like the hand-written plan:
// the cookie is set, the success text appears, and the typed password reaches no saved file.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
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
const secret = 'Pw-7f3k9-unique';
const page = `<!doctype html><title>Fixture</title><h1>Login</h1>
<form onsubmit="document.cookie='sid=abc123;path=/';out.textContent='Welcome '+pw.value.length;return false">
<input aria-label=User><input type=password id=pw aria-label=Password><button>Sign in</button></form><p id=out></p>`;
const files = dir =>
  readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)]
  );

test(
  'flow run login-form equals the hand-written plan and keeps the password off disk',
  { skip, timeout: 180000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-flow-real-'));
    const web = spawn(
      process.execPath,
      [
        '-e',
        `require('node:http').createServer((_, r) => { r.setHeader('content-type', 'text/html'); r.end(${JSON.stringify(page)}); }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['ignore', 'pipe', 'inherit'] }
    );
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
      return {
        ...(parsed.structuredContent ?? parsed),
        raw: r.stdout + r.stderr,
      };
    };
    // The page value of an expression, read from the saved CDP result.
    const evaluate = expression => {
      const out = spawnSync(
        process.execPath,
        [
          bin,
          '/raw',
          'cdp',
          'Runtime.evaluate',
          '--acknowledge',
          '--params',
          JSON.stringify({ expression, returnByValue: true }),
          '--port',
          String(port),
        ],
        { cwd, encoding: 'utf8' }
      ).stdout;
      const saved = /\[ARTIFACT\] cdp-1\.json (\S+)/.exec(out)?.[1];
      return JSON.parse(readFileSync(saved, 'utf8')).result.value;
    };
    try {
      assert.equal(cli('open', { port, headless: true, url }).ok, true);
      const params = {
        userLabel: 'User',
        userValue: 'bob',
        passwordLabel: 'Password',
        passwordValue: secret,
        submitName: 'Sign in',
        successText: 'Welcome',
      };
      const result = cli('flow', {
        action: 'run',
        name: 'login-form',
        params,
        connection: { port },
      });
      assert.ok(!result.raw.includes(secret), 'reply carries no secret');
      assert.equal(result.ok, true, result.raw);
      assert.equal(result.flow.requestedSteps, 3);
      assert.equal(result.flow.completedSteps, 3);
      assert.match(evaluate('document.cookie'), /sid=abc123/);
      assert.match(
        evaluate('document.getElementById("out").textContent'),
        new RegExp(`Welcome ${secret.length}`)
      );
      // Run folders and plans the engine wrote; the live browser profile is Chrome's own.
      for (const file of files(join(cwd, '.octocode')).filter(
        f => !f.includes('browser-state')
      ))
        if (statSync(file).size < 5e6)
          assert.ok(
            !readFileSync(file).includes(secret),
            `${file} persisted the password`
          );
    } finally {
      cli('cleanup', { port });
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
