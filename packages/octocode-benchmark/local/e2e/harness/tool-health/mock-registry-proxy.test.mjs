import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import tls from 'node:tls';
import { startRegistryProxy } from './mock-registry-proxy.mjs';

function tunnel(proxy, host) {
  return new Promise((resolve, reject) => {
    const req = http.request(proxy.proxyUrl, { method: 'CONNECT', path: `${host}:443` });
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
