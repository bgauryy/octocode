// The store is checked in: nothing but public provider data, the way the recorder
// keeps it. The secret detector's scan of the same files is a Rust test
// (crates/runtime/src/providers/artifact/test_http/recorded.rs).
import assert from 'node:assert/strict';
import test from 'node:test';
import { hasCase, readCase, replayer, requestKey } from './recorded-answers.mjs';
import { PLAN } from './recorded-plan.mjs';

const SAFE_HEADERS = new Set(['content-type', 'link', 'etag', 'cache-control', 'retry-after']);
const PERSON = /[A-Za-z0-9._%+-]+@(?!example\.test)[A-Za-z0-9-]+\.[A-Za-z]{2,}/;

test('every planned call has its recording', () => {
  assert.deepEqual(PLAN.filter(entry => !hasCase(entry.id)).map(entry => entry.id), []);
});

test('a recording holds no request header, no cookie, no private repository and no address', () => {
  for (const { id } of PLAN) {
    const recording = readCase(id);
    assert.ok(recording.exchanges.length, id);
    for (const exchange of recording.exchanges) {
      assert.deepEqual(Object.keys(exchange.request).filter(key => !['method', 'url', 'accept', 'body'].includes(key)), [], id);
      assert.deepEqual(exchange.headers.map(([name]) => name).filter(name => !SAFE_HEADERS.has(name)), [], id);
      assert.ok(/^sha256:[0-9a-f]{64}$/.test(exchange.digest), `${id}: the digest of the real answer`);
    }
    const text = JSON.stringify(recording);
    assert.doesNotMatch(text, /"private":\s*true/, id);
    assert.doesNotMatch(text, PERSON, id);
  }
});

test('replay answers by request identity and names what it does not hold', () => {
  const recording = { exchanges: [{ request: { method: 'POST', url: 'https://x.test/q', accept: 'a/b', body: { b: 1, a: 2 } }, status: 200, headers: [['content-type', 'application/json']], body: { ok: true } }] };
  const { answer, asked, unrecorded } = replayer(recording);
  const sent = (accept, body) => answer({ method: 'POST', headers: { accept } }, new URL('https://x.test/q'), body);
  assert.deepEqual(sent('a/b', '{"a":2,"b":1}').body, { ok: true }, 'key order is not identity');
  assert.equal(sent('a/b', '{"a":3,"b":1}').status, 404);
  assert.equal(sent('c/d', '{"a":2,"b":1}').status, 404);
  assert.equal(unrecorded.length, 2);
  assert.ok(asked.has(requestKey(recording.exchanges[0].request)));
});
