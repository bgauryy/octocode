import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from './helpers.ts';
import { promisify } from 'node:util';
import { realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { createServer as createSocketServer } from 'node:net';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, only, tempWorkspace, commandArgs, jsonCall, waitFor } from './helpers.ts';
import { runtime } from './store-support.ts';

/** The result as printed must satisfy the catalog's result schema of `command`. */
const conforms = (command: string, value: unknown): void => runtime.validation.validate(runtime.validation.asObject(runtime.catalog.catalog().results[command], `${command} result schema`), value);
const exec = promisify(execFile);
function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-service-');
  const database = join(workspace, 'audit.sqlite');
  const args = commandArgs(workspace, database);
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const asyncCall = async (command: string, input?: unknown, session?: string, env: NodeJS.ProcessEnv = {}): Promise<JsonRecord> => JSON.parse((await exec(binary, args(command, input, session), { env: { ...process.env, ...env }, timeout: 10000 })).stdout);
  const a = call('join', { name: 'sender', vendor: 'generic' }), b = call('join', { name: 'receiver', vendor: 'opencode' });
  const send = (body: string, wake = 'passive', key?: string) => call('send_message', { to: b.id, body, wake, key, reasoning: 'Verify native transport boundaries without a sender model' }, a.id);
  const db = new DatabaseSync(database); t.after(() => db.close());
  return { workspace, database, args, call, asyncCall, a, b, send, db };
}
type Fixture = ReturnType<typeof fixture>;
type Handler = (req: IncomingMessage, res: ServerResponse, body: JsonRecord) => void;
async function http(t: TestContext, workspace: string, handler: Handler, metadata?: (req: IncomingMessage, res: ServerResponse) => void) {
  const requests: JsonRecord[] = [], reads: string[] = [], directories: (string | null)[] = [], connections = new Set<number | undefined>();
  const server = createServer(async (req, res) => {
    connections.add(req.socket.remotePort);
    const url = new URL(req.url ?? '/', "http://localhost");
    directories.push(url.searchParams.get("directory"));
    req.url = url.pathname;
    if (req.method === 'GET') {
      reads.push(req.url);
      if (metadata) return metadata(req, res);
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(req.url === '/session/status' ? {} : { id: 'ses_existing', directory: workspace }));
      return;
    }
    const chunks: Buffer[] = []; for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString() || '{}'); requests.push({ method: req.method, url: req.url, body }); handler(req, res, body);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address();
  assert.ok(address && typeof address === 'object', 'The fixture listens on a TCP port');
  return { endpoint: `http://127.0.0.1:${address.port}`, requests, reads, directories, connections };
}
const attach = (f: Fixture, endpoint: string) => f.call('attach', { transport: 'opencode', endpoint, vendorSession: 'ses_existing' }, f.b.id);
test('OpenCode passive injection uses noReply, verifies receipt and never acknowledges handling', async t => {
  const f = fixture(t), server = await http(t, f.workspace, (_req, res, body) => { res.setHeader('content-type', 'application/json'); res.end(JSON.stringify({ info: { id: 'msg_serverAllocated', sessionID: 'ses_existing', role: 'user' }, parts: body.parts })); });
  const binding = attach(f, server.endpoint); conforms('attach', binding); assert.equal(binding.capabilities.passiveInjection, true); assert.equal(binding.capabilities.readReceipt, false);
  const sent = f.send('Single fact', 'passive', 'once'); f.send('Single fact', 'passive', 'once');
  const result = await f.asyncCall('dispatch', {}, f.b.id, { HTTP_PROXY: 'http://192.0.2.1:8', ALL_PROXY: 'http://192.0.2.1:8', NO_PROXY: '' });
  conforms('dispatch', result); assert.equal(result.submitted, 1); assert.equal(result.modelCalls, 0); assert.equal(result.recipientTurnRequested, false); assert.equal(result.receipt, 'http');
  assert.equal(server.requests[0].url, '/session/ses_existing/message'); assert.equal(server.requests[0].body.noReply, true); assert.equal(server.requests[0].body.parts[0].type, 'text'); assert.equal(server.requests[0].body.messageID, undefined, 'OpenCode allocates chronological message IDs');
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0); assert.equal(server.requests.length, 1); assert.equal(f.call('inbox', {}, f.b.id).items[0].id, sent.id);
});
test('OpenCode action explicitly requests existing-session inference without creating an agent', async t => {
  const f = fixture(t), server = await http(t, f.workspace, (_req, res) => { res.statusCode = 204; res.end(); }); attach(f, server.endpoint); f.send('Answer this question', 'action');
  const result = await f.asyncCall('dispatch', {}, f.b.id); conforms('dispatch', result); assert.equal(result.recipientTurnRequested, true); assert.equal(result.modelCalls, 0); assert.equal(server.requests[0].url, '/session/ses_existing/prompt_async'); assert.equal(server.requests[0].body.noReply, false); assert.equal(server.requests.length, 1);
});
test('OpenCode errors, redirects and wrong receipts stay uncertain and never replay automatically', async t => {
  for (const mode of ['http-error', 'redirect', 'wrong-id', 'wrong-session', 'wrong-text', 'dropped', 'oversized', 'timeout']) await t.test(mode, async t => {
    const f = fixture(t), server = await http(t, f.workspace, (_req, res) => {
      if (mode === 'dropped') { res.destroy(); return; }
      if (mode === 'timeout') return;
      if (mode === 'redirect') { res.writeHead(307, { location: 'http://192.0.2.1:8/leak' }); res.end(); return; }
      if (mode === 'http-error') { res.statusCode = 401; res.end('{}'); return; }
      res.setHeader('content-type', 'application/json'); res.end(mode === 'oversized' ? JSON.stringify({ padding: 'x'.repeat(1024 * 1024 + 1) }) : JSON.stringify({ info: { id: mode === 'wrong-id' ? 'wrong' : 'msg_serverAllocated', sessionID: mode === 'wrong-session' ? 'ses_wrong' : 'ses_existing', role: 'user' }, parts: [{ type: 'text', text: mode === 'wrong-text' ? 'Different body' : 'also incorrect' }] }));
    }); attach(f, server.endpoint); const sent = f.send('Preserve this'); await assert.rejects(() => f.asyncCall('dispatch', {}, f.b.id));
    assert.equal(only(f.db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id)).state, 'uncertain'); assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0); assert.equal(server.requests.length, 1); assert.equal(f.call('inbox', {}, f.b.id).items.length, 1);
  });
});
test('OpenCode endpoint validation rejects remote hosts, credentials, paths and query injection', t => {
  const f = fixture(t);
  for (const endpoint of ['http://example.com:4096', 'http://localhost:4096', 'https://127.0.0.1:4096', 'http://user:secret@127.0.0.1:4096', 'http://127.0.0.1:4096/session', 'http://127.0.0.1:4096/?x=1', 'http://127.0.0.1:4096/#x', 'http://127.0.0.1:0', 'unix:///tmp/a.sock']) assert.throws(() => attach(f, endpoint), /endpoint must be http:\/\/<loopback-IP>:<port>\/; no credentials, path, query or fragment/, endpoint);
});
test('Claude passive messages wait in DB until action mail authorizes a turn', { skip: process.platform === 'win32' }, async t => {
  const f = fixture(t), path = join(f.workspace, 'claude.sock'), frames: JsonRecord[] = [];
  const server = createSocketServer(stream => { let body = ''; stream.on('data', chunk => body += chunk); stream.on('end', () => { frames.push(JSON.parse(body)); stream.end(); }); });
  await new Promise<void>(resolve => server.listen(path, resolve)); t.after(() => server.close());
  const binding = f.call('attach', { transport: 'claude', endpoint: path, vendorSession: 'existing' }, f.b.id); assert.equal(binding.capabilities.passiveInjection, false); assert.equal(binding.capabilities.acceptanceReceipt, 'socket-write-only');
  f.send('FYI only'); assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0); assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 0);
  f.send('Question', 'action'); const result = await f.asyncCall('dispatch', {}, f.b.id); assert.equal(result.submitted, 2); assert.equal(result.recipientTurnRequested, true);
  await waitFor(() => frames.length >= 1); assert.equal(frames.length, 1); assert.match(frames[0].message.content, /FYI only/); assert.match(frames[0].message.content, /Question/); assert.equal(f.call('inbox', {}, f.b.id).items.length, 2);
});

test('Claude bounded action batch leaves excess passive mail available without another wake', { skip: process.platform === 'win32' }, async t => {
  const f = fixture(t), path = join(f.workspace, 'batch.sock');
  const server = createSocketServer(stream => { stream.resume(); stream.on('end', () => stream.end()); });
  await new Promise<void>(resolve => server.listen(path, resolve)); t.after(() => server.close());
  f.call('attach', { transport: 'claude', endpoint: path, vendorSession: 'existing' }, f.b.id);
  for (let i = 0; i < 18; i++) f.send(`FYI-${i}`);
  f.send('Action after FYIs', 'action');
  assert.equal((await f.asyncCall('dispatch', {}, f.b.id)).submitted, 16);
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0);
  assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 16);
  assert.equal(f.call('inbox', {}, f.b.id).items.length, 19, 'submitted and deferred mail remain available until handled');
  f.send('Next authorized action', 'action');
  assert.equal((await f.asyncCall('dispatch', {}, f.b.id)).submitted, 4);
  assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 20);
});

test('OpenCode rejects malformed vendor IDs at attachment time before any message is staged', t => {
  const f = fixture(t);
  for (const vendorSession of ['bad', 'ses_', 'ses_x/../../config', 'ses_x?query', 'ses_x#fragment', 'ses_' + 'x'.repeat(256)]) {
    assert.throws(() => f.call('attach', { transport: 'opencode', endpoint: 'http://127.0.0.1:4096', vendorSession }, f.b.id));
  }
  assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM attachments').get()).n, 0);
});


test('OpenCode preflight rejects a different workspace or session without staging mail', async t => {
  for (const mismatch of ['workspace', 'session', 'missing']) await t.test(mismatch, async t => {
    const f = fixture(t), server = await http(t, f.workspace, (_req, res) => { res.statusCode = 204; res.end(); }, (req, res) => {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(req.url === '/session/status' ? {} : mismatch === 'missing' ? {} : { id: mismatch === 'session' ? 'ses_wrong' : 'ses_existing', directory: mismatch === 'workspace' ? tmpdir() : f.workspace }));
    });
    attach(f, server.endpoint); f.send('Must stay queued', 'action');
    await assert.rejects(() => f.asyncCall('dispatch', {}, f.b.id));
    assert.equal(server.requests.length, 0);
    assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 0);
    assert.equal(f.call('inbox', {}, f.b.id).items.length, 1);
  });
});

test('OpenCode busy recipients defer without consuming a dispatch, then deliver at idle', async t => {
  const f = fixture(t); let status = 'busy';
  const server = await http(t, f.workspace, (_req, res) => { res.statusCode = 204; res.end(); }, (req, res) => {
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify(req.url === '/session/status' ? { ses_existing: { type: status } } : { id: 'ses_existing', directory: f.workspace }));
  });
  attach(f, server.endpoint); f.send('Wait for idle', 'action');
  const deferred = await f.asyncCall('dispatch', {}, f.b.id);
  assert.equal(deferred.deferred, 'recipient-not-idle');
  assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 0);
  status = 'idle';
  assert.equal((await f.asyncCall('dispatch', {}, f.b.id)).submitted, 1);
  assert.equal(server.requests.length, 1);
});

test('OpenCode authentication is endpoint-scoped and never stored in audit or output', async t => {
  const f = fixture(t), secret = 'probe-pass-only'; const headers: (string | undefined)[] = [];
  const expected = `Basic ${Buffer.from(`opencode:${secret}`).toString('base64')}`;
  const server = await http(t, f.workspace, (req, res) => { headers.push(req.headers.authorization); res.statusCode = 204; res.end(); }, (req, res) => {
    headers.push(req.headers.authorization);
    if (req.headers.authorization !== expected) { res.statusCode = 401; res.end('{}'); return; }
    res.setHeader('content-type', 'application/json'); res.end(JSON.stringify(req.url === '/session/status' ? {} : { id: 'ses_existing', directory: f.workspace }));
  });
  attach(f, server.endpoint); f.send('Authenticated message', 'action');
  const env = { OPENCODE_SERVER_PASSWORD: secret, OCTOCODE_OPENCODE_AUTH_ENDPOINT: server.endpoint };
  await assert.rejects(() => f.asyncCall('dispatch', {}, f.b.id, { ...env, OCTOCODE_OPENCODE_AUTH_ENDPOINT: 'http://127.0.0.1:1' }));
  assert.equal(headers.length, 0, 'Never send credentials to a different endpoint');
  assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 0);
  const result = await f.asyncCall('dispatch', {}, f.b.id, env);
  assert.equal(result.submitted, 1);
  assert.equal(headers.length, 3);
  assert.ok(headers.every(header => header === expected));
  assert.ok(!JSON.stringify(f.db.prepare('SELECT * FROM records').all()).includes(secret));
  assert.ok(!JSON.stringify(result).includes(secret));
});


test('OpenCode invalid preflight responses stay queued and never expose auth in errors', async t => {
  for (const response of [null, [], { ses_existing: { type: 'future-unknown' } }, { ses_existing: null }, 'unauthorized']) await t.test(JSON.stringify(response), async t => {
    const f = fixture(t), server = await http(t, f.workspace, (_req, res) => { res.statusCode = 204; res.end(); }, (req, res) => {
      if (response === 'unauthorized') { res.statusCode = 401; res.end('private-server-details'); return; }
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(req.url === '/session/status' ? response : { id: 'ses_existing', directory: f.workspace }));
    });
    attach(f, server.endpoint); f.send('Keep pending', 'action');
    await assert.rejects(() => f.asyncCall('dispatch', {}, f.b.id), (error: unknown) => error instanceof Error && !error.message.includes('private-server-details'));
    assert.equal(server.requests.length, 0);
    assert.equal(only(f.db.prepare('SELECT count(*) AS n FROM dispatches').get()).n, 0);
  });
});

test('OpenCode listener reuses its connection, checks each batch and does no idle HTTP polling', async t => {
  const f = fixture(t), server = await http(t, f.workspace, (_req, res) => { res.statusCode = 204; res.end(); });
  attach(f, server.endpoint); f.send('First batch', 'action');
  const listener = exec(binary, ['listen', ...f.args('listen', {}, f.b.id).slice(2), '--duration-ms', '1600'], { timeout: 10000 });
  await waitFor(() => server.requests.length > 0, { timeout: 5000 });
  assert.equal(server.requests.length, 1);
  f.send('Second batch', 'action');
  await listener;
  assert.equal(server.requests.length, 2);
  assert.deepEqual(server.reads, ['/session/ses_existing', '/session/status', '/session/ses_existing', '/session/status']);
  assert.equal(server.connections.size, 1, 'Reuse one HTTP connection across metadata and deliveries');
  assert.ok(server.directories.every(path => path === realpathSync(f.workspace)), 'Scope every request to the canonical recipient workspace');
  assert.equal(only(f.db.prepare("SELECT count(*) AS n FROM dispatches WHERE state='submitted'").get()).n, 2);
});
