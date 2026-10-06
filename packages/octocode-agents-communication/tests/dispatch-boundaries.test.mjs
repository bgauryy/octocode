import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from './helpers.mjs';
import { promisify } from 'node:util';
import { rmSync } from 'node:fs';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { createServer as socketServer } from 'node:net';
import { DatabaseSync } from 'node:sqlite';
import { nativeBinary as binary, tempDir, commandArgs, jsonCall } from './helpers.mjs';
const exec = promisify(execFile);
function fixture(t) {
  const workspace = tempDir('communication-boundaries-');
  const database = join(workspace, 'audit.sqlite');
  const args = commandArgs(workspace, database);
  const call = jsonCall(binary, workspace, database);
  const asyncCall = (command, input, session) => exec(binary, args(command, input, session), { timeout: 7500, killSignal: 'SIGKILL' });
  const sender = call('join', { name: 'sender', vendor: 'generic' });
  const receiver = call('join', { name: 'receiver', vendor: 'generic' });
  call('send_message', { to: receiver.id, body: 'Recipient-bound fact', reasoning: 'Exercise preflight boundaries', wake: 'passive' }, sender.id);
  const db = new DatabaseSync(database);
  t.after(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  return { workspace, call, asyncCall, receiver, db };
}
test('Codex requests have an absolute deadline even while a frame arrives slowly', async t => {
  const f = fixture(t), sockets = new Set(), timers = new Set();
  const server = socketServer(socket => {
    sockets.add(socket); socket.on('error', () => {});
    let timer;
    socket.once('data', request => {
      const key = request.toString().match(/sec-websocket-key: (.+)\r\n/i)[1];
      const accept = createHash('sha1').update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
      socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
      const payload = Buffer.from(JSON.stringify({ id: 1, result: { padding: 'x'.repeat(1000) } }));
      const header = Buffer.alloc(4); header[0] = 0x81; header[1] = 126; header.writeUInt16BE(payload.length, 2); socket.write(header);
      let offset = 0;
      timer = setInterval(() => { socket.write(payload.subarray(offset, ++offset)); if (offset === payload.length) clearInterval(timer); }, 75); timers.add(timer);
    });
    socket.on('close', () => { clearInterval(timer); timers.delete(timer); sockets.delete(socket); });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { for (const timer of timers) clearInterval(timer); for (const socket of sockets) socket.destroy(); server.close(); });
  f.call('attach', { transport: 'codex', endpoint: `ws://127.0.0.1:${server.address().port}`, vendorSession: 'existing-thread' }, f.receiver.id);
  const started = performance.now();
  await assert.rejects(() => f.asyncCall('dispatch', {}, f.receiver.id), error => {
    assert.match(error.stderr, /timed out/i);
    assert.equal(error.killed, false, 'adapter must terminate itself, not rely on the test timeout');
    return true;
  });
  assert.ok(performance.now() - started < 6500, 'the five-second deadline bounds partial frame input');
  assert.equal(f.db.prepare('SELECT count(*) AS n FROM dispatches').get().n, 0);
});
test('Changing the attachment during preflight prevents staging to the old recipient', async t => {
  const f = fixture(t), posts = [];
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost');
    res.setHeader('content-type', 'application/json');
    if (url.pathname === '/session/ses_existing') return res.end(JSON.stringify({ id: 'ses_existing', directory: f.workspace }));
    if (url.pathname === '/session/status') {
      f.call('attach', { transport: 'raw' }, f.receiver.id);
      return res.end('{}');
    }
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks)); posts.push(body);
    res.end(JSON.stringify({ info: { id: 'msg_fixture', sessionID: 'ses_existing', role: 'user' }, parts: body.parts }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  f.call('attach', { transport: 'opencode', endpoint: `http://127.0.0.1:${server.address().port}`, vendorSession: 'ses_existing' }, f.receiver.id);
  await assert.rejects(() => f.asyncCall('dispatch', {}, f.receiver.id), /attachment changed/i);
  assert.equal(posts.length, 0, 'old endpoint must not receive a newly staged body');
  assert.equal(f.db.prepare('SELECT count(*) AS n FROM dispatches').get().n, 0);
  assert.equal(f.call('inbox', {}, f.receiver.id).items.length, 1);
});
test('An in-flight native batch blocks attachment replacement until its receipt is recorded', async t => {
  const f = fixture(t); let blocked = false;
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost'); res.setHeader('content-type', 'application/json');
    if (req.method === 'GET') return res.end(JSON.stringify(url.pathname === '/session/status' ? {} : { id: 'ses_existing', directory: f.workspace }));
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks));
    assert.throws(() => f.call('attach', { transport: 'raw' }, f.receiver.id), /staged delivery/);
    blocked = true;
    res.end(JSON.stringify({ info: { id: 'msg_fixture', sessionID: 'ses_existing', role: 'user' }, parts: body.parts }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  f.call('attach', { transport: 'opencode', endpoint: `http://127.0.0.1:${server.address().port}`, vendorSession: 'ses_existing' }, f.receiver.id);
  assert.equal(JSON.parse((await f.asyncCall('dispatch', {}, f.receiver.id)).stdout).submitted, 1);
  assert.equal(blocked, true);
  assert.equal(f.call('attach', { transport: 'raw' }, f.receiver.id).transport, 'raw', 'receipt completion permits explicit rebinding');
});
