import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';
import {
  connectionKey,
  cacheConnection,
  getLiveConnection,
  evictConnection,
  touchConnection,
  listCDPSessions,
  closeAllChromeConnections,
  pruneClosedConnections,
  MAX_CACHED_CONNECTIONS,
} from '../src/chrome-connection-cache.js';
import type { ChromeConnection } from '../src/chrome-debug.js';

let closedCount = 0;

function fakeConnection(opts: { id?: string; url?: string; port?: number } = {}): ChromeConnection {
  let closed = false;
  return {
    session: {
      targetInfo: { id: opts.id ?? 'tgt-1', url: opts.url ?? 'https://x' },
      get closed() { return closed; },
      close() { closed = true; closedCount++; },
      send: async () => ({}),
      on() {}, off() {},
    },
    version: { Browser: 'Chrome/test' },
    metadata: { mode: 'attached', identity: { tabHost: 'x', tabPath: '/', cookieNames: [] }, activeTarget: {} },
    screenshotDir: '/tmp',
  } as unknown as ChromeConnection;
}

afterEach(() => { closeAllChromeConnections(); closedCount = 0; });

test('connectionKey is stable per port+target and distinct across targets', () => {
  assert.equal(connectionKey(9222, undefined), connectionKey(9222, undefined));
  assert.notEqual(connectionKey(9222, 'a'), connectionKey(9222, 'b'));
  assert.notEqual(connectionKey(9222, 'a'), connectionKey(9333, 'a'));
});

test('cache stores and returns a live connection; same object is reused', () => {
  const key = connectionKey(9222, undefined);
  const conn = fakeConnection();
  cacheConnection(key, 9222, conn);
  assert.strictEqual(getLiveConnection(key), conn, 'same connection reused (no new session)');
});

test('a closed connection is not returned and is pruned', () => {
  const key = connectionKey(9222, 'closed');
  const conn = fakeConnection();
  cacheConnection(key, 9222, conn);
  conn.session.close();
  assert.equal(getLiveConnection(key), undefined, 'closed session must not be reused');
});

test('evictConnection closes the session and removes it', () => {
  const key = connectionKey(9222, 'e');
  const conn = fakeConnection();
  cacheConnection(key, 9222, conn);
  evictConnection(key);
  assert.equal(conn.session.closed, true, 'evict closes the WS');
  assert.equal(getLiveConnection(key), undefined);
});

test('LRU cap evicts the least-recently-used connection (leak guard)', () => {
  const conns: ChromeConnection[] = [];
  for (let i = 0; i < MAX_CACHED_CONNECTIONS + 1; i++) {
    const c = fakeConnection({ id: `t${i}` });
    conns.push(c);
    cacheConnection(connectionKey(9000 + i, undefined), 9000 + i, c);
  }
  assert.equal(closedCount, 1, 'exactly one (the oldest) is closed when the cap is exceeded');
  assert.equal(conns[0]!.session.closed, true, 'the least-recently-used one was evicted');
  assert.equal(listCDPSessions().length, MAX_CACHED_CONNECTIONS);
});

test('touchConnection refreshes LRU ordering so the touched one survives', () => {
  const a = fakeConnection({ id: 'a' });
  const b = fakeConnection({ id: 'b' });
  cacheConnection(connectionKey(1, undefined), 1, a);
  cacheConnection(connectionKey(2, undefined), 2, b);
  touchConnection(connectionKey(1, undefined)); // a becomes most-recent
  // Add MAX-1 more so total = MAX+1 → exactly one eviction; 'b' (oldest) goes, 'a' survives.
  for (let i = 3; i < MAX_CACHED_CONNECTIONS + 2; i++) {
    cacheConnection(connectionKey(i, undefined), i, fakeConnection({ id: `t${i}` }));
  }
  assert.equal(b.session.closed, true, 'least-recently-used (b) is evicted');
  assert.equal(a.session.closed, false, 'recently touched connection survives eviction');
});

test('listCDPSessions reports metrics (port, uses, closed, ageMs)', () => {
  const key = connectionKey(9222, undefined);
  const conn = fakeConnection({ id: 'tgt-9', url: 'https://demo' });
  cacheConnection(key, 9222, conn);
  touchConnection(key);
  const sessions = listCDPSessions();
  assert.equal(sessions.length, 1);
  const s = sessions[0]!;
  assert.equal(s.port, 9222);
  assert.equal(s.targetId, 'tgt-9');
  assert.equal(s.closed, false);
  assert.ok(s.uses >= 1);
  assert.ok(typeof s.ageMs === 'number' && s.ageMs >= 0);
});

test('pruneClosedConnections removes dead entries', () => {
  const c1 = fakeConnection({ id: '1' });
  const c2 = fakeConnection({ id: '2' });
  cacheConnection(connectionKey(1, undefined), 1, c1);
  cacheConnection(connectionKey(2, undefined), 2, c2);
  c1.session.close();
  const removed = pruneClosedConnections();
  assert.equal(removed, 1);
  assert.equal(listCDPSessions().length, 1);
});

test('closeAllChromeConnections closes every session and empties the cache', () => {
  cacheConnection(connectionKey(1, undefined), 1, fakeConnection());
  cacheConnection(connectionKey(2, undefined), 2, fakeConnection());
  const n = closeAllChromeConnections();
  assert.equal(n, 2);
  assert.equal(listCDPSessions().length, 0);
});
