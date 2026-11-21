// NO_VISIBLE_EFFECT on a real isolated headless Chrome: a click whose after-condition is verified
// never reports it (even when the page answers asynchronously); an inert click still does.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

// The login answers after a delay, as a fetch would; #inert has no handler at all.
const page = `<form onsubmit="return false">
  <input id=u aria-label=User><input id=p aria-label=Pass type=password>
  <button id=go type=button onclick="setTimeout(() => { msg.textContent = 'Welcome back'; }, 600)">Sign in</button>
  <button id=inert type=button>Nothing</button></form><p id=msg></p>`;
const url = `data:text/html,${encodeURIComponent(page)}`;

test(
  'a click with a verified after-condition never reports NO_VISIBLE_EFFECT; an inert click does',
  { skip, timeout: 120000 },
  () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-effect-'));
    const port = freePort();
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      return `${r.stdout}${r.stderr}`;
    };
    return port.then(number => {
      const run = steps =>
        cli('run', {
          connection: { port: number },
          plan: { waitMs: 3000, steps },
        });
      try {
        cli('open', { port: number, headless: true, url: 'about:blank' });
        const login = (after, button = '#go') => [
          { op: 'goto', url },
          { op: 'act', action: 'fill', selector: '#u', value: 'alice' },
          { op: 'act', action: 'fill', selector: '#p', value: 'hunter2x' },
          {
            op: 'act',
            action: 'click',
            selector: button,
            ...(after ? { after } : {}),
          },
        ];
        for (const after of [
          { text: 'Welcome' },
          { selector: '#msg:not(:empty)' },
        ]) {
          const text = run(login(after));
          assert.ok(text.includes('"ok":true'), text.slice(0, 1500));
          assert.doesNotMatch(text, /NO_VISIBLE_EFFECT/, JSON.stringify(after));
        }
        // Without an after-condition the async answer is not awaited: the inert button is warned.
        const inert = run(login(undefined, '#inert'));
        assert.match(inert, /NO_VISIBLE_EFFECT/);
      } finally {
        cli('cleanup', { port: number });
        rmSync(cwd, { recursive: true, force: true });
      }
    });
  }
);
