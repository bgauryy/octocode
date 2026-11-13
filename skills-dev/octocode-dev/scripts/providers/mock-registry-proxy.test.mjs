import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import tls from 'node:tls';
import { startRegistryProxy } from './mock-registry-proxy.mjs';

function tunnel(proxy, host, credential) {
  return new Promise((resolve, reject) => {
    const headers = credential ? { 'proxy-authorization': `Basic ${Buffer.from(`caller:${credential}`).toString('base64')}` } : {};
    const req = http.request(proxy.proxyUrl, { method: 'CONNECT', path: `${host}:443`, headers });
    req.on('connect', (response, socket) => resolve({ response, socket }));
    req.on('error', reject);
    req.end();
  });
}
test('registry proxy validates TLS with its temporary CA and refuses unlisted hosts', async () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'registry-proxy-test-'));
  const proxy = await startRegistryProxy(tmp);
  try {
    const { response, socket } = await tunnel(proxy, 'pypi.org');
    assert.equal(response.statusCode, 200);
    const body = await new Promise((resolve, reject) => {
      const secure = tls.connect({ socket, servername: 'pypi.org', ca: fs.readFileSync(proxy.caFile) }, () => secure.write('GET /pypi/proof/json HTTP/1.1\r\nHost: pypi.org\r\nConnection: close\r\n\r\n'));
      let output = '';
      secure.on('data', chunk => { output += chunk; });
      secure.on('error', reject);
      secure.on('end', () => resolve(output));
    });
    assert.match(body, /^HTTP\/1\.1 200/);
    assert.match(body, /"version":"1.2.3"/);
    const denied = await tunnel(proxy, 'unknown.example');
    assert.equal(denied.response.statusCode, 403);
    denied.socket.destroy();
    assert.deepEqual(proxy.denied, ['unknown.example:443']);
    assert.deepEqual(proxy.unhandled, []);
  } finally { await proxy.close(); fs.rmSync(tmp, { recursive: true, force: true }); }
});

/** One GET through the proxy as `credential`: the status line and body. */
async function get(proxy, host, route, credential) {
  const { socket } = await tunnel(proxy, host, credential);
  return new Promise((resolve, reject) => {
    const secure = tls.connect({ socket, servername: host, ca: fs.readFileSync(proxy.caFile) }, () => secure.write(`GET ${route} HTTP/1.1\r\nHost: ${host}\r\nConnection: close\r\n\r\n`));
    let output = '';
    secure.on('data', chunk => { output += chunk; });
    secure.on('error', reject);
    secure.on('end', () => resolve(output));
  });
}

test('callers running at once neither see nor steer each other\'s provider world', async () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'registry-proxy-test-'));
  const proxy = await startRegistryProxy(tmp);
  try {
    const mine = proxy.scope(credential => credential === 'tool-a');
    assert.match(new URL(proxy.proxyUrlFor('tool-a')).password, /^tool-a$/);
    mine.failures.set('pypi.org', { status: 503, body: { error: 'Fixture outage' } });
    const [own, other, anonymous] = await Promise.all([
      get(proxy, 'pypi.org', '/pypi/proof/json', 'tool-a'),
      get(proxy, 'pypi.org', '/pypi/proof/json', 'tool-b'),
      get(proxy, 'pypi.org', '/pypi/proof/json'),
    ]);
    assert.match(own, /^HTTP\/1\.1 503/, 'a scope\'s injected failure answers its own caller');
    assert.match(other, /^HTTP\/1\.1 200/, 'and no other caller');
    assert.match(anonymous, /^HTTP\/1\.1 200/);
    assert.deepEqual(mine.requestsFrom, [{ request: 'GET https://pypi.org/pypi/proof/json', token: 'tool-a' }]);
    assert.deepEqual(proxy.requestsFrom.map(entry => entry.token).toSorted(), [null, 'tool-b'].toSorted(), 'callers no scope owns share the root world');
    const denied = await tunnel(proxy, 'unknown.example', 'tool-a');
    denied.socket.destroy();
    assert.deepEqual([mine.denied, proxy.denied], [['unknown.example:443'], []]);
  } finally { await proxy.close(); fs.rmSync(tmp, { recursive: true, force: true }); }
});
