// Test-only HTTPS proxy. It terminates TLS locally using an ephemeral CA and
// answers allowlisted registry hosts; it never opens an outbound connection.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import https from 'node:https';
import { spawnSync } from 'node:child_process';
import { OSV_HOST, REGISTRY_HOSTS, registryResponse } from './mock-routes-registry.mjs';
import { recordedJson } from './recorded-answers.mjs';

function certificates(tmp, hosts) {
  const dir = path.join(tmp, 'registry-tls');
  fs.mkdirSync(dir);
  const run = args => {
    const result = spawnSync('openssl', args, { cwd: dir, encoding: 'utf8', timeout: 15000 });
    if (result.status !== 0) throw new Error(`Unable to create hermetic registry certificate: ${result.stderr}`);
  };
  run(['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', 'ca.key', '-out', 'ca.pem', '-days', '1', '-subj', '/CN=Octocode hermetic test CA']);
  run(['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', 'server.key', '-out', 'server.csr', '-subj', '/CN=registry.test']);
  fs.writeFileSync(path.join(dir, 'extensions.cnf'), `basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=${hosts.map(host => `DNS:${host}`).join(',')}\n`);
  run(['x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key', '-CAcreateserial', '-out', 'server.pem', '-days', '1', '-extfile', 'extensions.cnf']);
  return { caFile: path.join(dir, 'ca.pem'), key: fs.readFileSync(path.join(dir, 'server.key')), cert: fs.readFileSync(path.join(dir, 'server.pem')) };
}

/**
 * `hosts`: what it serves. `answer(req, url, body)`, when given, answers every
 * request in place of the fixture routes: a recording, or the live provider.
 */
export async function startRegistryProxy(tmp, { hosts = REGISTRY_HOSTS, answer: answerAll } = {}) {
  const { caFile, key, cert } = certificates(tmp, hosts);
  const requests = [], denied = [], unhandled = [], responses = new Map(), failures = new Map();
  // What OSV received, and what it answers next; with none queued, its `{}`.
  const osvBodies = [], osvAnswers = [];
  const osvRecords = new Map([['GHSA-HAPPY', { body: { ...recordedJson('osv/advisory-ghsa', '/v1/vulns/'), id: 'GHSA-HAPPY' } }]]);
  const sockets = new Set();
  const answerFor = (req, url, sent) => {
    if (!REGISTRY_HOSTS.includes(url.hostname)) return null;
    if (url.hostname === OSV_HOST) {
      if (req.method === 'GET' && url.pathname.startsWith('/v1/vulns/')) return failures.get(OSV_HOST) ?? osvRecords.get(url.pathname.slice('/v1/vulns/'.length)) ?? { status: 404, body: { code: 5, message: 'Vulnerability not found' } };
      if (req.method !== 'POST' || url.pathname !== '/v1/query') return null;
      osvBodies.push(JSON.parse(sent));
      return failures.get(OSV_HOST) ?? osvAnswers.shift() ?? { body: {} };
    }
    if (req.method !== 'GET') return null;
    return failures.get(url.hostname) ?? responses.get(url.href) ?? registryResponse(url);
  };
  const tls = https.createServer({ key, cert }, (req, res) => {
    const chunks = [];
    req.on('data', chunk => chunks.push(chunk));
    req.on('end', async () => {
      const url = new URL(req.url, `https://${req.headers.host}`);
      requests.push(`${req.method} ${url.href}`);
      const sent = Buffer.concat(chunks).toString();
      const answer = answerAll ? await answerAll(req, url, sent) : answerFor(req, url, sent);
      if (!answer) unhandled.push(`${req.method} ${url.href}`);
      const payload = answer?.body ?? { error: 'Unmapped hermetic registry route' };
      const body = typeof payload === 'string' || Buffer.isBuffer(payload) ? payload : JSON.stringify(payload);
      res.writeHead(answer ? (answer.status ?? 200) : 502, { 'content-type': typeof payload === 'string' ? 'text/plain' : 'application/json', ...answer?.headers });
      res.end(body);
    });
  });
  const proxy = http.createServer((req, res) => { denied.push(req.url); res.writeHead(403); res.end(); });
  proxy.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  proxy.on('connect', (req, socket, head) => {
    const target = /^([^:]+):443$/.exec(req.url);
    if (!target || !hosts.includes(target[1])) { denied.push(req.url); socket.end('HTTP/1.1 403 Forbidden\r\n\r\n'); return; }
    socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
    if (head.length) socket.unshift(head);
    tls.emit('connection', socket);
  });
  await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
  return {
    proxyUrl: `http://127.0.0.1:${proxy.address().port}`, caFile, requests, denied, unhandled, responses, failures, osvBodies, osvAnswers, osvRecords,
    close: () => new Promise(resolve => { for (const socket of sockets) socket.destroy(); proxy.close(resolve); }),
  };
}
