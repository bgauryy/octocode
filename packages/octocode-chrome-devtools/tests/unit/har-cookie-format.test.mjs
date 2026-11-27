// Cookie evidence in HAR entries: pure, from recorded extra-info events.
import test from 'node:test';
import assert from 'node:assert/strict';
import { harDocument } from '../../dist/engine/cdp-checks/live-har-format.mjs';

const VALUE = 'cookie-secret-91';
const record = extras => ({
  requestId: 'r1',
  method: 'POST',
  url: 'http://x.test/login',
  start: 1,
  complete: true,
  end: 2,
  request: { method: 'POST', url: 'http://x.test/login', headers: { A: 'b' } },
  response: { status: 302, headers: { Location: '/home' } },
  hop: 0,
  extras,
});
const entryOf = extras =>
  harDocument(new Map([['k', record(extras)]]), {
    performanceSnapshot: {},
    monitorMs: 1,
    title: '',
  }).log.entries[0];

test('Set-Cookie and Cookie header names stay, values are redacted, cookie names are listed', () => {
  const entry = entryOf({
    request: [
      {
        headers: { Cookie: `old=${VALUE}` },
        associatedCookies: [
          {
            blockedReasons: ['SameSiteLax'],
            cookie: { name: 'old', value: VALUE, domain: 'x.test', path: '/' },
          },
        ],
      },
    ],
    response: [
      {
        statusCode: 302,
        headers: { 'Set-Cookie': `sid=${VALUE}; Path=/app\nb=2` },
        blockedCookies: [
          { blockedReasons: ['SecureOnly'], cookieLine: `b=${VALUE}` },
        ],
      },
    ],
  });
  const set = entry.response.headers.find(h => h.name === 'Set-Cookie');
  assert.match(set.value, /^\[redacted \d+ chars\]$/);
  assert.match(
    entry.request.headers.find(h => h.name === 'Cookie').value,
    /^\[redacted \d+ chars\]$/
  );
  assert.deepEqual(
    entry['_cookies'].map(c => [c.direction, c.name, c.path, c.blockedReasons]),
    [
      ['request', 'old', '/', ['SameSiteLax']],
      ['response', 'sid', '/app', []],
      ['response', 'b', null, ['SecureOnly']],
    ]
  );
  assert.ok(!JSON.stringify(entry).includes(VALUE));
});

test('a response extra of another hop is not merged and no extras means no cookies', () => {
  const other = entryOf({
    request: [],
    response: [{ statusCode: 200, headers: { 'Set-Cookie': `z=${VALUE}` } }],
  });
  assert.deepEqual(other['_cookies'], []);
  assert.deepEqual(entryOf(undefined)['_cookies'], []);
});
