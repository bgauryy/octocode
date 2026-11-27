// Test-only HTTPS proxy. It terminates TLS locally using an ephemeral CA and
// answers allowlisted registry hosts; it never opens an outbound connection.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import tls from 'node:tls';
import { spawnSync } from 'node:child_process';
import { OSV_HOST, REGISTRY_HOSTS, registryResponse } from './mock-routes-registry.mjs';
import { recordedJson } from './recorded-answers.mjs';

/** The `modified` stamp of the first content a queued record states none for. */
const OSV_MODIFIED = Date.parse('2024-01-01T00:00:00Z');

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

/** The credential a client put in its proxy URL (`http://caller:<credential>@host:port`), or null. */
function callerOf(req) {
  // The credential starts at the first non-space, so the scheme's spaces and the credential never overlap.
  const basic = /^Basic\s+(\S.*)$/i.exec(req.headers['proxy-authorization'] ?? '')?.[1];
  if (!basic) return null;
  const pair = Buffer.from(basic, 'base64').toString('utf8');
  const colon = pair.indexOf(':');
  return colon < 0 ? null : decodeURIComponent(pair.slice(colon + 1));
}

/**
 * What one world of callers sees and steers: its requests (and who sent each),
 * refusals, unhandled routes, injected answers and failures, and OSV's queue
 * and records. `answerAll(req, url, body)`, when given, answers in place of the
 * fixture routes.
 */
function providerWorld(answerAll) {
  const requests = [], requestsFrom = [], denied = [], unhandled = [], responses = new Map(), failures = new Map();
  // What OSV received, and what it answers next; with none queued, its `{}`.
  const osvBodies = [], osvAnswers = [];
  const osvRecords = new Map([['GHSA-happy', { body: { ...recordedJson('osv/advisory-ghsa', '/v1/vulns/'), id: 'GHSA-happy' } }]]);
  // Each id's latest content and the stamp it was given. OSV changes a record's
  // `modified` whenever the record changes, and the engine keeps a record under
  // (id, modified) for good: a row that queues other content for an id it used
  // before must get a new stamp, or it would be answered with the old record.
  const osvStamps = new Map();
  const osvStamp = record => {
    if (record.modified) return record.modified;
    const content = JSON.stringify(record);
    const seen = osvStamps.get(record.id);
    if (seen?.content === content) return seen.modified;
    const modified = new Date(seen ? Date.parse(seen.modified) + 1000 : OSV_MODIFIED).toISOString().replace('.000Z', 'Z');
    osvStamps.set(record.id, { content, modified });
    return modified;
  };
  /**
   * OSV's `/v1/querybatch`: each query takes the next queued answer and lists its
   * records by id and `modified`, as OSV does; the records themselves are served by
   * `GET /v1/vulns/{id}` from `osvRecords`, so a row has one source of truth. An
   * answer with a status, or a body that is not an object, answers the request whole.
   */
  const osvBatch = queries => {
    osvBodies.push(...queries);
    const results = [];
    for (const _ of queries) {
      const answer = osvAnswers.shift() ?? { body: {} };
      if ((answer.status ?? 200) !== 200 || typeof answer.body !== 'object') return answer;
      const listed = (answer.body.vulns ?? []).map(record => {
        const stored = { ...record, modified: osvStamp(record) };
        osvRecords.set(stored.id, { body: stored });
        return { id: stored.id, modified: stored.modified };
      });
      results.push({ ...(listed.length && { vulns: listed }), ...(answer.body.next_page_token && { next_page_token: answer.body.next_page_token }) });
    }
    return { body: { results } };
  };
  const fixtureAnswer = (req, url, sent) => {
    if (!REGISTRY_HOSTS.includes(url.hostname)) return null;
    if (url.hostname === OSV_HOST) {
      if (req.method === 'GET' && url.pathname.startsWith('/v1/vulns/')) return failures.get(OSV_HOST) ?? osvRecords.get(url.pathname.slice('/v1/vulns/'.length)) ?? { status: 404, body: { code: 5, message: 'Vulnerability not found' } };
      if (req.method !== 'POST' || url.pathname !== '/v1/querybatch') return null;
      return failures.get(OSV_HOST) ?? osvBatch(JSON.parse(sent).queries);
    }
    if (req.method !== 'GET') return null;
    return failures.get(url.hostname) ?? responses.get(url.href) ?? registryResponse(url);
  };
  return {
    state: { requests, requestsFrom, denied, unhandled, responses, failures, osvBodies, osvAnswers, osvRecords },
    answer: (req, url, sent) => (answerAll ? answerAll(req, url, sent) : fixtureAnswer(req, url, sent)),
  };
}

/**
 * `hosts`: what it serves. `answer(req, url, body)`, when given, answers every
 * request in place of the fixture routes: a recording, or the live provider.
 * The returned object is the world of every caller no scope owns. `scope(owns)`
 * opens another world for the callers `owns(credential)` accepts, with its own
 * requests, failures and OSV state, so callers running at the same time never
 * read or steer each other's; `proxyUrlFor(credential)` is the proxy URL that
 * names a caller.
 */
export async function startRegistryProxy(tmp, { hosts = REGISTRY_HOSTS, answer: answerAll } = {}) {
  const { caFile, key, cert } = certificates(tmp, hosts);
  const root = providerWorld(answerAll);
  const scopes = [];
  const worldOf = caller => scopes.find(scope => scope.owns(caller))?.world ?? root;
  const sockets = new Set();
  const callers = new WeakMap();
  const decrypted = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', chunk => chunks.push(chunk));
    req.on('end', async () => {
      const caller = callers.get(req.socket) ?? null;
      const { state, answer: answerOf } = worldOf(caller);
      const url = new URL(req.url, `https://${req.headers.host}`);
      state.requests.push(`${req.method} ${url.href}`);
      state.requestsFrom.push({ request: state.requests.at(-1), token: caller });
      const sent = Buffer.concat(chunks).toString();
      const answer = await answerOf(req, url, sent);
      if (!answer) state.unhandled.push(`${req.method} ${url.href}`);
      const payload = answer?.body ?? { error: 'Unmapped hermetic registry route' };
      const body = typeof payload === 'string' || Buffer.isBuffer(payload) ? payload : JSON.stringify(payload);
      res.writeHead(answer ? (answer.status ?? 200) : 502, { 'content-type': typeof payload === 'string' ? 'text/plain' : 'application/json', ...answer?.headers });
      res.end(body);
    });
  });
  const proxy = http.createServer((req, res) => { worldOf(callerOf(req)).state.denied.push(req.url); res.writeHead(403); res.end(); });
  proxy.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  proxy.on('connect', (req, socket, head) => {
    const caller = callerOf(req);
    const target = /^([^:]+):443$/.exec(req.url);
    if (!target || !hosts.includes(target[1])) { worldOf(caller).state.denied.push(req.url); socket.end('HTTP/1.1 403 Forbidden\r\n\r\n'); return; }
    socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
    if (head.length) socket.unshift(head);
    const secure = new tls.TLSSocket(socket, { isServer: true, key, cert, ALPNProtocols: ['http/1.1'] });
    callers.set(secure, caller);
    decrypted.emit('connection', secure);
  });
  await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
  const proxyUrl = `http://127.0.0.1:${proxy.address().port}`;
  return {
    proxyUrl, caFile, ...root.state,
    proxyUrlFor: credential => `http://caller:${encodeURIComponent(credential)}@127.0.0.1:${proxy.address().port}`,
    scope: owns => {
      const world = providerWorld(answerAll);
      scopes.push({ owns, world });
      return { proxyUrl, caFile, ...world.state };
    },
    close: () => new Promise(resolve => { for (const socket of sockets) socket.destroy(); proxy.close(resolve); }),
  };
}
