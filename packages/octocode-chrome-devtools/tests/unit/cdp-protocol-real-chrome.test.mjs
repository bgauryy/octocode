// Dynamic CDP on a real isolated headless Chrome: live-schema lookup and search, local validation,
// the central safety policy for the cdp command, plan steps and runner scripts.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readdirSync,
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

test(
  'protocol lookup, search, local validation and the policy work against the installed Chrome',
  { skip, timeout: 240000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-protocol-'));
    const port = await freePort();
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const both = r => r.stdout + r.stderr;
    try {
      const opened = engine('open', '--headless', '--port', String(port));
      assert.equal(opened.status, 0, both(opened));
      const P = ['--port', String(port)];

      const member = engine('protocol', 'DOM.querySelector', ...P);
      assert.equal(member.status, 0, both(member));
      const described = JSON.parse(member.stdout);
      assert.deepEqual(described.params.map(p => p.name).sort(), [
        'nodeId',
        'selector',
      ]);
      assert.match(described.docsUrl, /\/tot\/DOM\/#method-querySelector$/);
      assert.equal(described.policy, 'free');
      const cacheDir = join(
        cwd,
        '.octocode/octocode-chrome-devtools/tmp/protocol-cache'
      );
      const [cached] = readdirSync(cacheDir);
      assert.equal(statSync(join(cacheDir, cached)).mode & 0o777, 0o600);

      const domain = JSON.parse(engine('protocol', 'Emulation', ...P).stdout);
      assert.ok(
        domain.methods.some(
          m => m.name === 'setGeolocationOverride' && m.policy === 'needsAck'
        )
      );

      const search = engine('protocol', '--search', 'geolocation', ...P);
      const found = JSON.parse(search.stdout);
      assert.ok(
        found.hits.some(h => h.name === 'Emulation.setGeolocationOverride')
      );
      assert.equal(found.total >= found.hits.length, true);

      const typo = engine('protocol', 'DOM.querySelectr', ...P);
      assert.notEqual(typo.status, 0);
      assert.match(both(typo), /DOM\.querySelector/);

      // A wrong param type is refused locally with the expected shape.
      const wrong = engine(
        'cdp',
        'DOM.querySelector',
        '--params',
        '{"nodeId":"x","selector":1}',
        ...P
      );
      assert.notEqual(wrong.status, 0);
      assert.match(
        both(wrong),
        /\[INVALID\].*nodeId: expected integer.*selector: expected string/s
      );
      assert.match(both(wrong), /Expected params:.*nodeId: DOM\.NodeId/);
      assert.match(both(wrong), /#method-querySelector/);

      // A method with a heavy consequence is refused without --acknowledge and never reaches the browser.
      const closed = engine('cdp', 'Browser.close', '--browser', ...P);
      assert.notEqual(closed.status, 0);
      assert.match(
        both(closed),
        /Browser\.close .*every tab.*user's permission.*acknowledge: true/s
      );
      assert.equal(engine('targets', ...P).status, 0);

      // needsAck: refused without acknowledge before any browser call, sent with it.
      const grant = engine(
        'cdp',
        'Browser.grantPermissions',
        '--params',
        '{"permissions":["geolocation"]}',
        '--browser',
        ...P
      );
      assert.notEqual(grant.status, 0);
      assert.match(both(grant), /user's permission.*acknowledge: true/);
      assert.doesNotMatch(grant.stdout, /\[ARTIFACT\]/);
      const sent = engine('cdp', 'Network.getCookies', '--acknowledge', ...P);
      assert.equal(sent.status, 0, both(sent));
      assert.match(sent.stdout, /\[ARTIFACT\]/);

      // A plan refuses a literal method without permission at validation: no browser is needed (unused port).
      const offline = String(await freePort());
      const plan = engine(
        'run',
        '--json',
        JSON.stringify({ steps: [{ op: 'cdp', method: 'Page.crash' }] }),
        '--port',
        offline
      );
      assert.notEqual(plan.status, 0);
      assert.match(both(plan), /Page\.crash .*user's permission/);
      assert.doesNotMatch(both(plan), /No Chrome|ECONNREFUSED/i);

      // A script is raw except for the hard-stop set: it cannot ask the user, so it cannot acknowledge.
      writeFileSync(
        join(cwd, '.octocode/close.mjs'),
        `export async function run(cdp) {
  try { await cdp.send('Browser.close'); console.log('SENT'); }
  catch (error) { console.log('REFUSED ' + error.message); }
}\n`
      );
      const script = engine('script', '.octocode/close.mjs', ...P);
      assert.match(
        both(script),
        /REFUSED .*Browser\.close is refused in scripts/
      );
      assert.doesNotMatch(both(script), /SENT/);
      assert.equal(engine('targets', ...P).status, 0);
    } finally {
      engine('cleanup', '--port', String(port));
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);

test(
  'Browser.close with acknowledge really closes a throwaway browser',
  { skip, timeout: 120000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-close-'));
    const port = await freePort();
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const listening = () =>
      fetch(`http://127.0.0.1:${port}/json/version`, {
        signal: AbortSignal.timeout(1000),
      }).then(
        () => true,
        () => false
      );
    try {
      assert.equal(
        engine('open', '--headless', '--port', String(port)).status,
        0
      );
      assert.equal(await listening(), true);
      const closed = engine(
        'cdp',
        'Browser.close',
        '--browser',
        '--acknowledge',
        '--port',
        String(port)
      );
      assert.doesNotMatch(closed.stdout + closed.stderr, /\[POLICY\]/);
      for (let tries = 0; tries < 50 && (await listening()); tries++)
        await new Promise(resolve => setTimeout(resolve, 100));
      assert.equal(
        await listening(),
        false,
        'the acknowledged close ended the browser'
      );
    } finally {
      engine('cleanup', '--port', String(port));
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
