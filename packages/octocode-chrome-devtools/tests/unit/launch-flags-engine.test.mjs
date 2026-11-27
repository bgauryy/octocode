// Typed launch flags at engine level: the shared denylist is enforced even when the engine is
// called directly (/raw), features join once, extras follow the managed flags, and a reused or
// attached browser never takes launch flags silently.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';
import { chromeFlagDenylist } from '@octocodeai/config/chrome';
import { engineOpenEntry, skip, startHarness } from './coverage-fixture.mjs';

const { chromeLaunchArgs, validateChromeFlags, appliedLaunchFlags } =
  await import(
    join(import.meta.dirname, '../../dist/engine/browser-launch-config.mjs')
  );

const launch = overrides =>
  chromeLaunchArgs({
    port: '9222',
    userDataDir: '/tmp/octo-launch-flags',
    isolated: true,
    profile: 'Default',
    headless: true,
    os: 'linux',
    userAgent: '',
    windowSize: '',
    enableFeatures: '',
    proxy: { server: '', bypassList: '', pacUrl: '' },
    url: 'http://example.test/',
    ...overrides,
  });

/** A concrete flag for a denylist entry: prefix entries refuse anything that starts with them. */
const sample = denied => (denied.prefix ? `${denied.flag}x` : denied.flag);

test('every denylisted flag is refused by name and reason, from the exported list', () => {
  assert.ok(chromeFlagDenylist.length > 0);
  for (const denied of chromeFlagDenylist) {
    const entry = `${sample(denied)}=1`;
    assert.throws(
      () => validateChromeFlags([entry]),
      message(sample(denied), denied)
    );
    assert.throws(
      () => launch({ chromeFlags: [entry] }),
      message(sample(denied), denied)
    );
  }
});
const message = (name, denied) => error =>
  error.message.includes(name) && error.message.includes(denied.reason);

test('malformed flags, repeated names and managed duplicates are refused', () => {
  for (const bad of ['mute-audio', '--Upper', '--a b', '--a=b,c', ''])
    assert.throws(
      () => validateChromeFlags([bad]),
      /not a valid Chrome flag/,
      bad
    );
  assert.throws(() => validateChromeFlags(['--a=1', '--a=2']), /twice/);
  assert.deepEqual(validateChromeFlags(['--a=1', '--a=1']), ['--a=1']);
  const managed = launch({}).filter(
    arg => arg.startsWith('--') && !arg.includes('=')
  );
  assert.ok(managed.length > 0);
  for (const flag of managed)
    assert.throws(
      () => launch({ chromeFlags: [flag] }),
      /already sets it/,
      flag
    );
});

test('feature lists join into one switch each; extras follow every managed flag and precede the URL', () => {
  const args = launch({
    enableFeatures: 'A, B',
    disableFeatures: 'C,D,C',
    blinkFeatures: 'E',
    chromeFlags: ['--mute-audio', '--foo=bar'],
  });
  const once = name => args.filter(arg => arg.startsWith(`${name}=`));
  assert.deepEqual(once('--disable-features'), ['--disable-features=C,D']);
  assert.deepEqual(once('--enable-blink-features'), [
    '--enable-blink-features=E',
  ]);
  assert.equal(once('--enable-features').length, 1);
  assert.deepEqual(args.slice(-3), [
    '--mute-audio',
    '--foo=bar',
    'http://example.test/',
  ]);
  const without = launch({});
  assert.ok(!without.some(arg => arg.startsWith('--disable-features')));
  assert.ok(!without.some(arg => arg.startsWith('--enable-blink-features')));
});

test('appliedFlags lists the extras and proxy presence only, never the proxy server', () => {
  const reply = appliedLaunchFlags({
    chromeFlags: ['--mute-audio'],
    enableFeatures: 'A',
    disableFeatures: 'B',
    blinkFeatures: 'C',
    userAgent: 'secret-agent/1',
    windowSize: '800x600',
    proxyRequested: true,
  });
  assert.equal(reply.proxy, true);
  for (const flag of [
    '--mute-audio',
    '--enable-features=A',
    '--disable-features=B',
    '--enable-blink-features=C',
    '--user-agent',
    '--window-size=800,600',
  ])
    assert.ok(reply.appliedFlags.includes(flag), flag);
  assert.ok(!JSON.stringify(reply).includes('secret-agent'));
  assert.deepEqual(
    appliedLaunchFlags({
      chromeFlags: [],
      enableFeatures: '',
      disableFeatures: '',
      blinkFeatures: '',
      userAgent: '',
      windowSize: '',
      proxyRequested: false,
    }),
    { appliedFlags: [] }
  );
});

const engineOpen = (cwd, ...args) =>
  spawnSync(process.execPath, [engineOpenEntry, ...args], {
    cwd,
    encoding: 'utf8',
  });

test('the engine refuses denylisted flags and attach with launch flags when called directly', async () => {
  const cwd = mkdtempSync(join(tmpdir(), 'octo-flags-'));
  const port = String(await freePort());
  try {
    for (const denied of chromeFlagDenylist) {
      const r = engineOpen(
        cwd,
        '--port',
        port,
        '--chrome-flags',
        sample(denied)
      );
      assert.notEqual(r.status, 0, sample(denied));
      const out = r.stdout + r.stderr;
      assert.ok(
        out.includes(sample(denied)) && out.includes(denied.reason),
        out
      );
    }
    const attach = engineOpen(
      cwd,
      '--port',
      port,
      '--attach',
      '--acknowledge',
      '--chrome-flags',
      '--mute-audio'
    );
    assert.notEqual(attach.status, 0);
    assert.match(
      attach.stdout + attach.stderr,
      /--attach never launches[^]*--chrome-flags/
    );
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});

test(
  'a fresh launch reports appliedFlags and the proxy only as presence; a reused browser reports launchFlagsIgnored',
  { skip, timeout: 180000 },
  async () => {
    const h = await startHarness(undefined, {}, [
      '--url',
      'about:blank',
      '--chrome-flags',
      '--mute-audio',
      '--disableFeatures',
      'OptimizationHints',
      '--blinkFeatures',
      'Serial',
      '--userAgent',
      'octo-flags-agent/1.0',
      '--windowSize',
      '900x700',
      '--proxyServer',
      'http://user:pw@127.0.0.1:9',
      '--proxyBypassList',
      '*',
    ]);
    try {
      assert.equal(h.opened.ok, true, JSON.stringify(h.opened));
      const text = JSON.stringify(h.opened);
      for (const flag of [
        '--mute-audio',
        '--disable-features=OptimizationHints',
        '--enable-blink-features=Serial',
        '--user-agent',
        '--window-size=900,700',
      ])
        assert.ok(text.includes(flag), `${flag} in ${text}`);
      assert.ok(text.includes('"proxy":true'));
      assert.ok(!text.includes('user:pw'), 'proxy credentials never echoed');
      const version = await fetch(
        `http://127.0.0.1:${h.port}/json/version`
      ).then(r => r.json());
      assert.equal(version['User-Agent'], 'octo-flags-agent/1.0');

      const again = await h.engineOpen('--chrome-flags', '--mute-audio');
      assert.equal(again.ok, true, JSON.stringify(again));
      assert.ok(
        JSON.stringify(again).includes('"launchFlagsIgnored":true'),
        JSON.stringify(again)
      );
      const plain = await h.engineOpen();
      assert.ok(!JSON.stringify(plain).includes('launchFlagsIgnored'));
    } finally {
      await h.stop();
    }
  }
);
