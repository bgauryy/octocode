// Navigation to local-file and browser-internal URLs needs the user's consent, and deprecated twins
// of guarded methods are guarded too, on a real isolated headless Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';
import { loadSchema } from '../../dist/engine/cdp-schema.mjs';
import {
  CDP_POLICY_TABLE,
  classify,
  twinEntries,
  URL_SCHEME_POLICY,
} from '../../dist/engine/cdp-policy.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

test(
  'file: and internal URLs are refused without consent and deprecated twins inherit the policy',
  { skip, timeout: 240000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-real-urlpolicy-'));
    const port = await freePort();
    const P = ['--port', String(port)];
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const both = r => r.stdout + r.stderr;
    const plan = steps =>
      engine('run', '--json', JSON.stringify({ steps }), ...P);
    try {
      const opened = engine('open', '--headless', '--port', String(port));
      assert.equal(opened.status, 0, both(opened));

      for (const { scheme } of URL_SCHEME_POLICY) {
        const url = scheme === 'about' ? 'about:version' : `${scheme}:///x`;
        const goto = plan([{ op: 'goto', url }]);
        assert.notEqual(goto.status, 0, scheme);
        assert.match(
          both(goto),
          new RegExp(`goto: ${scheme}: URLs are refused`)
        );
        assert.match(both(goto), /acknowledge: true/);
        assert.doesNotMatch(both(goto), /\[ARTIFACT\] (CDP|STEPS).*cdp-1/);
      }
      const hosts = 'file:///etc/hosts';
      const navigate = engine(
        'cdp',
        'Page.navigate',
        '--params',
        JSON.stringify({ url: hosts }),
        ...P
      );
      assert.notEqual(navigate.status, 0);
      assert.match(
        both(navigate),
        /Page\.navigate: file: URLs are refused.*read files/
      );
      const create = engine(
        'cdp',
        'Target.createTarget',
        '--params',
        JSON.stringify({ url: hosts }),
        ...P
      );
      assert.match(
        both(create),
        /Target\.createTarget: file: URLs are refused/
      );
      const tab = engine('targets', '--new-tab', hosts, ...P);
      assert.match(both(tab), /--new-tab: file: URLs are refused/);
      const openUrl = engine('open', '--headless', '--url', hosts, ...P);
      assert.notEqual(openUrl.status, 0);
      assert.match(both(openUrl), /--url: file: URLs are refused/);

      // Consent through cdp opens the door; data: and about:blank never needed it.
      const consented = engine(
        'cdp',
        'Page.navigate',
        '--params',
        JSON.stringify({ url: 'data:text/html,<p>ok</p>' }),
        ...P
      );
      assert.equal(consented.status, 0, both(consented));
      const consent = engine(
        'cdp',
        'Page.navigate',
        '--acknowledge',
        '--params',
        JSON.stringify({ url: hosts }),
        ...P
      );
      assert.equal(consent.status, 0, both(consent));

      // A deprecated twin of a guarded method needs consent too.
      const twin = engine(
        'cdp',
        'Page.setGeolocationOverride',
        '--params',
        '{"latitude":1,"longitude":1,"accuracy":1}',
        ...P
      );
      assert.notEqual(twin.status, 0);
      assert.match(both(twin), /user's permission.*acknowledge: true/);

      // Every twin the installed Chrome declares is covered by the offline table too.
      const schema = await loadSchema(port, {
        cacheDir: join(cwd, 'schema-cache'),
      });
      const table = new Set(CDP_POLICY_TABLE.map(entry => entry.method));
      for (const method of twinEntries(schema).keys())
        assert.ok(table.has(method), `${method} missing from the static table`);
      assert.notEqual(classify('Page.setGeolocationOverride'), 'free');
    } finally {
      engine('cleanup', ...P);
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
