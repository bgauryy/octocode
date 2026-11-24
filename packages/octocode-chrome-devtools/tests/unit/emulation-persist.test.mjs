// `open --emulation` persists per port and the runner re-applies it on every later call (real Chrome).
import { DIE_WITH_PARENT } from '../fixtures/die-with-parent.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
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

const PROBE = `JSON.stringify({
  width: innerWidth,
  dark: matchMedia('(prefers-color-scheme: dark)').matches,
  tz: Intl.DateTimeFormat().resolvedOptions().timeZone,
  lang: navigator.language,
  coarse: matchMedia('(pointer: coarse)').matches,
})`;

const FULL = {
  width: 412,
  height: 800,
  colorScheme: 'dark',
  timezone: 'Europe/Paris',
  locale: 'fr-FR',
  cpuThrottle: 2,
  touch: true,
};

test(
  'stored emulation reaches later separate calls, survives reuse, resets, and ports are independent',
  { skip, timeout: 240000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-emu-'));
    const web = spawn(
      process.execPath,
      [
        ...DIE_WITH_PARENT,
        '-e',
        `require('node:http').createServer((_, r) => { r.setHeader('content-type', 'text/html'); r.end('<!doctype html><title>x</title><p>hi</p>'); }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['pipe', 'pipe', 'inherit'] }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
    const [a, b] = [await freePort(), await freePort()];
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
    const metaOf = port =>
      join(
        cwd,
        '.octocode/octocode-chrome-devtools/tmp/browser-state',
        `cdp-session-meta-${port}.json`
      );
    const probe = port => {
      const out = cli('cdp', {
        connection: { port },
        method: 'Runtime.evaluate',
        params: { expression: PROBE, returnByValue: true },
        acknowledge: true,
      });
      const text = JSON.stringify(out);
      const found = /\{\\"width\\":[^}]*\}/.exec(text);
      assert.ok(found, text);
      return JSON.parse(found[0].replaceAll('\\"', '"'));
    };
    const open = (port, extra = {}) =>
      cli('open', { port, headless: true, url, ...extra });
    try {
      const opened = open(a, { emulation: FULL });
      assert.equal(opened.ok, true, JSON.stringify(opened));
      assert.deepEqual(
        JSON.parse(readFileSync(metaOf(a), 'utf8')).emulation,
        FULL
      );
      const text = JSON.stringify(opened);
      assert.match(text, /persistent/);
      for (const call of [probe(a), probe(a)]) {
        assert.equal(call.width, FULL.width);
        assert.equal(call.dark, true);
        assert.equal(call.tz, FULL.timezone);
        assert.equal(call.lang, FULL.locale);
        assert.equal(call.coarse, true);
      }
      // A check recipe is a runner call too.
      const check = cli('run', {
        connection: { port: a },
        plan: {
          steps: [
            {
              op: 'cdp',
              method: 'Runtime.evaluate',
              acknowledge: true,
              params: { expression: PROBE, returnByValue: true },
            },
          ],
        },
      });
      assert.match(JSON.stringify(check), /width.{1,4}412/);

      // Port b is independent.
      assert.equal(open(b).ok, true);
      assert.equal(
        JSON.parse(readFileSync(metaOf(b), 'utf8')).emulation,
        undefined
      );
      assert.notEqual(probe(b).width, FULL.width);

      // A reuse without --emulation keeps it; a new --emulation replaces it.
      assert.equal(open(a).ok, true);
      assert.equal(probe(a).width, FULL.width);
      assert.equal(
        open(a, { emulation: { width: 500, colorScheme: 'light' } }).ok,
        true
      );
      const replaced = probe(a);
      assert.equal(replaced.width, 500);
      assert.equal(replaced.dark, false);
      assert.equal(replaced.tz === FULL.timezone, false);

      // Reset removes it.
      const reset = open(a, { emulation: { reset: true } });
      assert.equal(reset.ok, true);
      assert.match(JSON.stringify(reset), /reset/);
      assert.equal(
        JSON.parse(readFileSync(metaOf(a), 'utf8')).emulation,
        undefined
      );
      assert.notEqual(probe(a).width, 500);

      // Cleanup clears it: a new browser starts without one.
      assert.equal(open(a, { emulation: FULL }).ok, true);
      cli('cleanup', { port: a });
      assert.equal(existsSync(metaOf(a)), false);
      assert.equal(open(a).ok, true);
      assert.notEqual(probe(a).width, FULL.width);
    } finally {
      for (const port of [a, b])
        try {
          cli('cleanup', { port });
        } catch {
          // The browser may already be gone: cleanup is best effort.
        }
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);

test(
  'geolocation needs acknowledge and, once given, reaches navigator.geolocation',
  { skip, timeout: 120000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-emu-geo-'));
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
    const raw = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', 'open', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const web = spawn(
      process.execPath,
      [
        ...DIE_WITH_PARENT,
        '-e',
        `require('node:http').createServer((_, r) => { r.setHeader('content-type', 'text/html'); r.end('<!doctype html><p>hi</p>'); }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['pipe', 'pipe', 'inherit'] }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const geo = { latitude: 48.85, longitude: 2.35, accuracy: 10 };
    const metaDir = join(
      cwd,
      '.octocode/octocode-chrome-devtools/tmp/browser-state'
    );
    try {
      const refused = raw(
        '--port',
        String(port),
        '--headless',
        '--emulation',
        JSON.stringify({ geolocation: geo })
      );
      assert.notEqual(refused.status, 0);
      assert.match(refused.stdout + refused.stderr, /acknowledge/);
      assert.equal(
        existsSync(metaDir) &&
          readdirSync(metaDir).some(f => f.includes('meta')),
        false
      );
      for (const bad of [
        { geolocation: { latitude: 91, longitude: 0 } },
        { geolocation: { latitude: 0, longitude: -181 } },
        { geolocation: { latitude: 0, longitude: 0, accuracy: -1 } },
        { cpuThrottle: 0.5 },
        { width: 0 },
        { height: 10.5 },
        { deviceScaleFactor: 0 },
      ]) {
        const r = raw(
          '--port',
          String(port),
          '--acknowledge',
          '--emulation',
          JSON.stringify(bad)
        );
        assert.notEqual(r.status, 0, JSON.stringify(bad));
      }
      assert.equal(
        existsSync(metaDir) &&
          readdirSync(metaDir).some(f => f.includes('meta')),
        false
      );

      const ok = cli('open', {
        port,
        headless: true,
        url: `http://127.0.0.1:${webPort}/`,
        emulation: { geolocation: geo },
        acknowledge: true,
      });
      assert.equal(ok.ok, true, JSON.stringify(ok));
      assert.match(JSON.stringify(ok), /geolocation: set/);
      assert.equal(JSON.stringify(ok).includes('48.85'), false);
      // Permission grants last as long as the connection that made them: grant and read in one run.
      const got = cli('run', {
        connection: { port },
        plan: {
          steps: [
            {
              op: 'cdp',
              method: 'Browser.grantPermissions',
              acknowledge: true,
              params: {
                permissions: ['geolocation'],
                origin: `http://127.0.0.1:${webPort}`,
              },
            },
            {
              op: 'cdp',
              method: 'Runtime.evaluate',
              acknowledge: true,
              params: {
                expression:
                  'new Promise(r => navigator.geolocation.getCurrentPosition(p => r(JSON.stringify([p.coords.latitude, p.coords.longitude])), e => r("err:" + e.message)))',
                awaitPromise: true,
                returnByValue: true,
              },
            },
          ],
        },
      });
      assert.match(JSON.stringify(got), /48\.85,2\.35/);
    } finally {
      try {
        cli('cleanup', { port });
      } catch {
        // The browser may already be gone: cleanup is best effort.
      }
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
