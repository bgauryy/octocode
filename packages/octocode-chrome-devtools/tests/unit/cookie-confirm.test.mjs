import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import test from 'node:test';

const engine = resolve(import.meta.dirname, '../../dist/engine');
const { parseConfirmDomains, cookieDomainConfirmed } = await import(
  resolve(engine, 'cookie-confirm.mjs')
);

test('confirm-domains: empty, wildcard and bare-TLD entries are refused', () => {
  for (const raw of [
    '',
    ' , ',
    '*',
    '*.example.com',
    'com',
    'https://a.com',
    'a.com:443',
  ])
    assert('error' in parseConfirmDomains(raw), raw);
});

test('confirm-domains: named domains parse, normalized and deduplicated', () => {
  assert.deepEqual(
    parseConfirmDomains('.Example.com, example.com,auth.example.com'),
    {
      domains: ['example.com', 'auth.example.com'],
    }
  );
});

test('confirm-domains: a cookie moves only for a confirmed domain or its subdomain', () => {
  const d = ['example.com'];
  assert(cookieDomainConfirmed({ domain: '.example.com' }, d));
  assert(cookieDomainConfirmed({ domain: 'a.example.com' }, d));
  assert(!cookieDomainConfirmed({ domain: 'notexample.com' }, d));
  assert(!cookieDomainConfirmed({ domain: 'example.com.evil.io' }, d));
  assert(!cookieDomainConfirmed({}, d));
});

test('cookie-bridge: --from-port with the ack but no --confirm-domains is refused', () => {
  const r = spawnSync(
    process.execPath,
    [
      resolve(engine, 'cookie-bridge.mjs'),
      '--i-understand-secrets',
      '--from-port',
      '9333',
      '--to-port',
      '9222',
    ],
    { encoding: 'utf8', timeout: 30_000 }
  );
  assert.equal(r.status, 1);
  assert.match(r.stderr, /--confirm-domains/);
});

test('cookie-bridge: a wildcard --confirm-domains is refused', () => {
  const r = spawnSync(
    process.execPath,
    [
      resolve(engine, 'cookie-bridge.mjs'),
      '--i-understand-secrets',
      '--from-port',
      '9333',
      '--to-port',
      '9222',
      '--confirm-domains',
      '*',
    ],
    { encoding: 'utf8', timeout: 30_000 }
  );
  assert.equal(r.status, 1);
  assert.match(r.stderr, /wildcards/);
});

test('public suffixes, trailing dots and IDN forms are handled when confirming domains', async () => {
  for (const suffix of ['co.uk', 'com.au', 'ac.jp'])
    assert.ok(parseConfirmDomains(suffix).error, suffix);
  assert.deepEqual(parseConfirmDomains('example.co.uk').domains, [
    'example.co.uk',
  ]);
  assert.deepEqual(parseConfirmDomains('example.com.').domains, [
    'example.com',
  ]);
  const idn = parseConfirmDomains('bücher.example').domains;
  assert.equal(
    cookieDomainConfirmed({ domain: 'xn--bcher-kva.example' }, idn),
    true
  );
  assert.equal(
    cookieDomainConfirmed({ domain: 'evilexample.com' }, ['example.com']),
    false
  );
});
