import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const skill = join(import.meta.dirname, '../..');
const engine = join(skill, 'dist/engine');
const {
  cookieKey,
  cookieMeta,
  filterCookies,
  readStorageStateCookies,
  toSetCookie,
  toStorageStateCookie,
} = await import(join(engine, 'cookie-jar.mjs'));

const partitioned = {
  name: 'part',
  value: 'SECRET-PART',
  domain: 'embed.example',
  path: '/',
  expires: -1,
  size: 15,
  httpOnly: false,
  secure: true,
  session: true,
  sameSite: 'None',
  priority: 'High',
  sourceScheme: 'Secure',
  sourcePort: 443,
  partitionKey: {
    topLevelSite: 'https://top.example',
    hasCrossSiteAncestor: true,
  },
};
const hostOnly = {
  name: 'sid',
  value: 'SECRET-SID',
  domain: 'app.example.org',
  path: '/',
  expires: 1900000000,
  httpOnly: true,
  secure: true,
  session: false,
  sameSite: 'Strict',
};
const domainWide = {
  name: 'pref',
  value: 'SECRET-PREF',
  domain: '.example.org',
  path: '/a',
  expires: -1,
  httpOnly: false,
  secure: false,
  session: true,
};

test('toSetCookie keeps scope, flags, expiry and partition', () => {
  assert.deepEqual(toSetCookie(partitioned), {
    name: 'part',
    value: 'SECRET-PART',
    domain: 'embed.example',
    path: '/',
    secure: true,
    httpOnly: false,
    sameSite: 'None',
    priority: 'High',
    sourceScheme: 'Secure',
    sourcePort: 443,
    partitionKey: {
      topLevelSite: 'https://top.example',
      hasCrossSiteAncestor: true,
    },
  });
  const set = toSetCookie(hostOnly);
  assert.equal(set.domain, 'app.example.org', 'host-only domain stays dotless');
  assert.equal(set.expires, 1900000000);
  assert.equal(set.sameSite, 'Strict');
  assert.equal(set.httpOnly, true);
  assert.equal(
    'expires' in toSetCookie(domainWide),
    false,
    'session stays session'
  );
  assert.equal(
    'sameSite' in toSetCookie(domainWide),
    false,
    'unspecified stays unspecified'
  );
});

test('storage state uses the Playwright shape and round-trips partitions', () => {
  const state = [partitioned, hostOnly, domainWide].map(toStorageStateCookie);
  assert.deepEqual(state[0], {
    name: 'part',
    value: 'SECRET-PART',
    domain: 'embed.example',
    path: '/',
    expires: -1,
    httpOnly: false,
    secure: true,
    sameSite: 'None',
    partitionKey: 'https://top.example',
    _crHasCrossSiteAncestor: true,
  });
  assert.equal(state[2].sameSite, 'Lax', 'Playwright default for unspecified');
  const back = readStorageStateCookies({ cookies: state });
  assert.deepEqual(
    back.map(cookieKey),
    [partitioned, hostOnly, domainWide].map(cookieKey)
  );
  assert.deepEqual(toSetCookie(back[0]).partitionKey, partitioned.partitionKey);
  // Playwright files without the Chromium flag mean a cross-site partition.
  const [pw] = readStorageStateCookies({
    cookies: [{ ...state[0], _crHasCrossSiteAncestor: undefined }],
  });
  assert.equal(toSetCookie(pw).partitionKey.hasCrossSiteAncestor, true);
  assert.deepEqual(readStorageStateCookies([hostOnly]).length, 1);
  assert.throws(() => readStorageStateCookies({ origins: [] }), /cookies/);
});

test('filterCookies follows host-only and domain matching', () => {
  const jar = [partitioned, hostOnly, domainWide];
  const names = urls => filterCookies(jar, urls).map(c => c.name);
  assert.deepEqual(names([]), ['part', 'sid', 'pref']);
  assert.deepEqual(names(['https://app.example.org/x']), ['sid', 'pref']);
  assert.deepEqual(
    names(['https://sub.app.example.org/']),
    ['pref'],
    'host-only is exact'
  );
  assert.deepEqual(names(['https://example.org/']), ['pref']);
  assert.deepEqual(names(['https://other.test/']), []);
});

test('cookie metadata and the bridge never carry values', () => {
  for (const c of [partitioned, hostOnly, domainWide])
    assert.equal(JSON.stringify(cookieMeta(c)).includes('SECRET'), false);
  assert.equal(cookieMeta(partitioned).partitioned, true);
  const refused = spawnSync(
    process.execPath,
    [join(engine, 'cookie-bridge.mjs'), '--to-port', '1', '--from-port', '2'],
    { encoding: 'utf8' }
  );
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /i-understand-secrets/);
  const same = spawnSync(
    process.execPath,
    [
      join(engine, 'cookie-bridge.mjs'),
      '--i-understand-secrets',
      '--confirm-domains',
      'example.com',
      '--from-profile',
      'Default',
      '--to-port',
      '9333',
    ],
    { encoding: 'utf8' }
  );
  assert.equal(same.status, 1);
  assert.match(same.stderr, /--source-port must differ/);
});
