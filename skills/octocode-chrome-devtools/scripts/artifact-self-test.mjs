#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log('Usage: artifact-self-test.mjs\n\nChecks paginated artifact queries, filter-preserving continuations, and retained network evidence without Chrome.');
  process.exit(0);
}
const scripts = dirname(fileURLToPath(import.meta.url));
const root = mkdtempSync(join(tmpdir(), 'octo artifacts '));
const checks = join(scripts, 'cdp-checks');
const count = 73;
function json(file, value) { writeFileSync(file, JSON.stringify(value)); }
function cli(name, args) {
  const result = spawnSync(process.execPath, [join(checks, name), ...args], { cwd: root, encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
function resume(next) {
  const result = spawnSync(next.command, next.args, { cwd: root, encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
function cdp(outputDir, navigate) {
  const handlers = new Map();
  const session = {
    outputDir, targetInfo: { url: 'https://example.test/page' },
    on(event, fn) { handlers.set(event, fn); },
    off(event, fn) { if (handlers.get(event) === fn) handlers.delete(event); },
    emit(event, data, meta = {}) { handlers.get(event)?.(data, meta); },
    async send(method, params) {
      if (method === 'Page.navigate') navigate(session);
      if (method === 'Network.getResponseBody') return { body: 'x'.repeat(5000), base64Encoded: false };
      return {};
    },
  };
  return session;
}
const originalLog = console.log;
try {
  const snapshotFile = join(root, 'page snapshot.json');
  const expectedRefs = Array.from({ length: count }, (_, i) => 'e' + (i + 1));
  json(snapshotFile, { refs: Object.fromEntries(expectedRefs.map(ref => [ref, { role: 'button', name: 'same label' }])) });
  let snapshotPage = cli('snapshot-query.mjs', ['--file', snapshotFile, '--limit', '9']);
  const refRows = [];
  for (;;) {
    refRows.push(...snapshotPage.rows.map(r => r.ref));
    if (!snapshotPage.next) break;
    snapshotPage = resume(snapshotPage.next.continue);
  }
  assert.deepEqual(refRows, expectedRefs);

  const net = join(root, 'network-measure.json');
  const rows = Array.from({ length: count }, (_, i) => ({ url: `https://example.test/api/${i}`, kind: 'api', ms: 2000, status: 503 }));
  json(net, { counts: { requests: count, failed: count, slow: count }, failures: rows, slow: rows, sample: rows });
  let result = cli('measure-query.mjs', ['--net', net, '--view', 'failures', '--limit', '11', '--kind', 'api', '--domain', 'example.test']);
  const found = [];
  for (;;) {
    found.push(...result.failures.map((row) => row.url));
    assert.equal(result.pagination.failures.totalRows, count);
    if (!result.next.continue) break;
    result = resume(result.next.continue);
  }
  assert.deepEqual(found, rows.map((row) => row.url));

  const storage = join(root, 'storage-measure.json');
  const keys = Array.from({ length: count }, (_, i) => `key-${i}`);
  json(storage, { storage: { localStorageKeys: keys, cacheNames: keys.slice(0, 35) } });
  result = cli('measure-query.mjs', ['--storage', storage, '--view', 'keys', '--limit', '10']);
  const locals = [], caches = [];
  for (;;) {
    locals.push(...result.keys.local);
    caches.push(...result.keys.caches);
    if (!result.next.continue) break;
    result = resume(result.next.continue);
  }
  assert.deepEqual(locals, keys);
  assert.deepEqual(caches, keys.slice(0, 35));

  const har = join(root, 'network capture.har');
  json(har, { log: { entries: rows.map((row, i) => ({ request: { url: row.url, method: 'GET' }, response: { status: i % 2 ? 200 : 503 }, _resourceType: i % 3 ? 'Fetch' : 'Script' })) } });
  result = cli('har-pager.mjs', [har, '--format', 'json', '--page-size', '7', '--kind', 'Fetch', '--status', '503', '--url-regex', '/api/']);
  const indices = [];
  for (;;) {
    indices.push(...result.pageRows.map((row) => row.index));
    if (!result.next.continue) break;
    result = resume(result.next.continue);
  }
  assert.deepEqual(indices, rows.flatMap((_, i) => i % 2 === 0 && i % 3 !== 0 ? [i] : []));

  for (const name of ['har-pager.mjs', 'har-redact.mjs']) {
    const help = spawnSync(process.execPath, [join(checks, name), '--help'], { cwd: root, encoding: 'utf8' });
    assert.equal(help.status, 0, help.stderr);
  }

  // Exercise the event boundary with more responses than the old artifact caps.
  console.log = () => {};
  const networkDir = join(root, 'network');
  mkdirSync(networkDir);
  const network = await import(join(checks, 'network-measure-check.mjs'));
  await network.run(cdp(networkDir, (session) => {
    for (let i = 0; i < count; i++) {
      session.emit('Network.requestWillBeSent', { requestId: String(i), request: { url: rows[i].url, method: 'GET' }, type: 'Fetch' });
      session.emit('Network.responseReceived', { requestId: String(i), response: { status: 503, mimeType: 'application/json' } });
    }
  }));
  const captured = JSON.parse(readFileSync(join(networkDir, 'network-measure.json')));
  assert.equal(captured.failures.length, count);
  assert.equal(captured.sample.length, count);

  const bodyDir = join(root, 'bodies');
  mkdirSync(bodyDir);
  const bodies = await import(join(checks, 'network-body-har-fetch-check.mjs'));
  await bodies.run(cdp(bodyDir, (session) => {
    for (let i = 0; i < count; i++) {
      const requestId = String(i);
      session.emit('Network.requestWillBeSent', { requestId, request: { url: `https://example.test/api/data/${i}`, method: 'GET' }, type: 'Fetch' });
      session.emit('Network.responseReceived', { requestId, response: { status: 200, mimeType: 'application/json' } });
      session.emit('Network.loadingFinished', { requestId });
    }
  }));
  const capturedBodies = JSON.parse(readFileSync(join(bodyDir, 'network-bodies.json')));
  const bodyHar = JSON.parse(readFileSync(join(bodyDir, 'network-body.har')));
  assert.equal(capturedBodies.length, count);
  assert.equal(bodyHar.log.entries.length, count);
  assert(bodyHar.log.entries.every((entry) => entry.response.content.text.length === 5000));

  // Request IDs belong to sessions: iframe and page IDs can collide.
  process.env.MONITOR_MS = '10';
  const monitor = await import(join(checks, 'live-har-monitor.mjs'));
  for (const [name, module, artifact] of [
    ['session-network', network, 'network-measure.json'],
    ['session-bodies', bodies, 'network-body.har'],
    ['session-monitor', monitor, 'live-network.har'],
  ]) {
    const dir = join(root, name); mkdirSync(dir);
    const emit = session => {
      for (const sessionId of [undefined, 'iframe-session']) {
        const meta = { sessionId }, requestId = 'same-id';
        session.emit('Network.requestWillBeSent', { requestId, request: { url: 'https://example.test/api/data/' + (sessionId || 'page'), method: 'GET' }, type: 'Fetch' }, meta);
        session.emit('Network.responseReceived', { requestId, response: { status: 200, mimeType: 'application/json' } }, meta);
        session.emit('Network.loadingFinished', { requestId }, meta);
      }
    };
    const session = cdp(dir, emit);
    await module.run(session, name === 'session-monitor' ? { onReady: () => emit(session) } : undefined);
    const captured = JSON.parse(readFileSync(join(dir, artifact)));
    const rows = captured.sample || captured.log.entries;
    assert.equal(rows.length, 2, name + ' must retain both sessions');
    assert.equal(new Set(rows.map(row => row.url || row.request.url)).size, 2);
  }

} finally {
  console.log = originalLog;
  rmSync(root, { recursive: true, force: true });
}
console.log(JSON.stringify({ ok: true, suite: 'chrome-devtools-artifacts', checks: 8, rows: count }));
