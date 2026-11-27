import test from 'node:test';
import assert from 'node:assert/strict';
import { hostIdentity } from '../../dist/engine/undercover.mjs';

const MAC_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/155.0.0.0 Safari/537.36';
const WINDOWS_UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36';
const ANDROID_UA =
  'Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Mobile Safari/537.36';
const LINUX_UA =
  'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36';

const brands = [
  { brand: 'HeadlessChrome', version: '155' },
  { brand: 'Chromium', version: '155' },
  { brand: 'Not(A:Brand', version: '24' },
];
// What the probe reads on a macOS host: navigator plus getHighEntropyValues.
const probe = userAgent => ({
  userAgent,
  platform: 'MacIntel',
  metadata: {
    architecture: 'arm',
    bitness: '64',
    brands,
    formFactors: ['Desktop'],
    fullVersionList: [],
    mobile: false,
    model: '',
    platform: 'macOS',
    platformVersion: '15.4.0',
    wow64: false,
  },
});

test('the host user agent keeps the host navigator and client hints, minus headless', () => {
  const identity = hostIdentity(probe(MAC_UA));
  assert.match(identity.userAgent, / Chrome\/155/);
  assert.equal(identity.platform, 'MacIntel');
  assert.equal(identity.userAgentMetadata.platform, 'macOS');
  assert.equal(identity.userAgentMetadata.platformVersion, '15.4.0');
  assert.equal(identity.userAgentMetadata.architecture, 'arm');
  assert.deepEqual(
    identity.userAgentMetadata.brands.map(b => b.brand),
    ['Google Chrome', 'Chromium', 'Not(A:Brand']
  );
});

test('a custom Windows user agent on another host gets Windows client hints and the host navigator.platform', () => {
  const identity = hostIdentity(probe(WINDOWS_UA));
  assert.equal(identity.userAgent, WINDOWS_UA);
  assert.equal(identity.platform, 'MacIntel');
  const hints = identity.userAgentMetadata;
  assert.equal(hints.platform, 'Windows');
  assert.equal(hints.mobile, false);
  assert.equal(hints.architecture, 'x86');
  assert.equal(hints.bitness, '64');
  assert.equal(hints.platformVersion, '10.0.0');
  assert.equal(hints.wow64, false);
  assert.deepEqual(hints.formFactors, ['Desktop']);
  assert.equal(hints.brands.length, 3);
});

test('a custom Android phone user agent reports mobile hints and the device model', () => {
  const identity = hostIdentity(probe(ANDROID_UA));
  assert.equal(identity.platform, 'MacIntel');
  const hints = identity.userAgentMetadata;
  assert.equal(hints.platform, 'Android');
  assert.equal(hints.mobile, true);
  assert.equal(hints.model, 'Pixel 7');
  assert.equal(hints.platformVersion, '13');
  assert.deepEqual(hints.formFactors, ['Mobile']);
  assert.equal(hints.architecture, '');
});

test('a custom Linux user agent gets Linux hints', () => {
  const identity = hostIdentity(probe(LINUX_UA));
  assert.equal(identity.platform, 'MacIntel');
  assert.equal(identity.userAgentMetadata.platform, 'Linux');
  assert.equal(identity.userAgentMetadata.architecture, 'x86');
  assert.equal(identity.userAgentMetadata.platformVersion, '');
});

test('a custom user agent for the host OS is kept with the host hints', () => {
  const own = MAC_UA.replace('HeadlessChrome', 'Chrome').replace(
    '155.0.0.0',
    '154.0.0.0'
  );
  const identity = hostIdentity(probe(own));
  assert.equal(identity.userAgent, own);
  assert.equal(identity.platform, 'MacIntel');
  assert.equal(identity.userAgentMetadata.platformVersion, '15.4.0');
});

test('a user agent naming no known OS drops the client hints it cannot match', () => {
  const identity = hostIdentity(probe('OctocodeBot/1.0'));
  assert.equal(identity.userAgent, 'OctocodeBot/1.0');
  assert.equal(identity.userAgentMetadata, undefined);
});
