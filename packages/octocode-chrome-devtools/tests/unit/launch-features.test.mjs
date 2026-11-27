import test from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';

const { chromeLaunchArgs } = await import(
  join(import.meta.dirname, '../../dist/engine/browser-launch-config.mjs')
);

const launch = overrides =>
  chromeLaunchArgs({
    port: '9222',
    userDataDir: '/tmp/octo-launch-features',
    isolated: true,
    profile: 'Default',
    headless: true,
    os: 'linux',
    userAgent: '',
    windowSize: '',
    enableFeatures: '',
    proxy: { server: '', bypassList: '', pacUrl: '' },
    url: '',
    ...overrides,
  });

// A repeated --enable-features switch can replace the earlier one, so one switch carries them all.
const features = args => {
  const switches = args.filter(arg => arg.startsWith('--enable-features='));
  assert.equal(switches.length, 1, args.join(' '));
  return switches[0].slice('--enable-features='.length).split(',');
};

test('every fresh launch enables WebMCP, so pages expose document.modelContext and CDP lists their tools', () => {
  for (const options of [{}, { headless: false }, { isolated: false }])
    assert.ok(features(launch(options)).includes('WebMCP'));
});

test('requested features join WebMCP in the one switch, each once', () => {
  assert.deepEqual(
    features(launch({ enableFeatures: 'Foo, WebMCP,,Bar' })).toSorted(),
    ['Bar', 'Foo', 'WebMCP']
  );
});

test('headless Linux disables the Chrome sandbox only when running as root', () => {
  const sandboxOff = launch({}).includes('--no-sandbox');
  assert.equal(sandboxOff, process.getuid?.() === 0);
  assert.ok(!launch({ os: 'darwin' }).includes('--no-sandbox'));
});
