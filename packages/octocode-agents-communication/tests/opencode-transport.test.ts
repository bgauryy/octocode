// The OpenCode native transport against a fake loopback HTTP API: preflight, receipts, deferral and credential binding.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import type { TestContext } from 'node:test';
import type { JsonRecord } from './json-record.ts';
import { runtime } from './store-support.ts';
import { listening, portOf, rejects, workspace } from './transport-support.ts';

const { transport } = runtime;

/** A fake OpenCode HTTP API over loopback; `calls` records every request. */
async function opencodeServer(t: TestContext, ws: string, { status = 'idle', directory = ws, message }: { status?: string; directory?: string; message?: JsonRecord } = {}) {
  const calls: JsonRecord[] = [];
  const server = createServer((request, response) => {
    const chunks: Buffer[] = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : null;
      calls.push({ method: request.method, url: request.url, authorization: request.headers.authorization, body });
      const url = request.url ?? '';
      const json = (code: number, value: unknown) => { response.writeHead(code, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(value)); };
      if (url.startsWith('/session/status')) return json(200, status ? { ses_a: { type: status } } : {});
      if (url.startsWith('/session/ses_a/prompt_async')) { response.writeHead(204); return response.end(); }
      if (url.startsWith('/session/ses_a/message')) return json(200, message ?? { info: { sessionID: 'ses_a', role: 'user', id: 'msg_1' }, parts: [{ type: 'text', text: body.parts[0].text }] });
      if (url.startsWith('/session/ses_a')) return json(200, { id: 'ses_a', directory });
      response.writeHead(404); response.end();
    });
  });
  t.after(() => { server.closeAllConnections(); server.close(); });
  await listening(server, 0, '127.0.0.1');
  return { endpoint: `http://127.0.0.1:${portOf(server)}`, calls };
}

test('opencode: preflight, passive message receipt, action prompt and the directory query', async t => {
  const ws = workspace(t);
  const { endpoint, calls } = await opencodeServer(t, ws);
  const delivery = await transport.NativeDelivery.open('opencode', endpoint, 'ses_a', ws);
  t.after(() => delivery.close());
  assert.equal(await delivery.prepare(), true);
  assert.deepEqual(await delivery.offer('fyi text', 'tok', false), { kind: 'http', turnRequested: false });
  assert.deepEqual(await delivery.offer('act text', 'tok', true), { kind: 'http', turnRequested: true });
  const message = calls.find(call => call.url.startsWith('/session/ses_a/message'));
  assert.ok(message);
  assert.deepEqual(message.body, { noReply: true, parts: [{ type: 'text', text: 'fyi text' }] });
  assert.equal(new URL(endpoint + message.url).searchParams.get('directory'), ws);
  const prompted = calls.find(call => call.url.startsWith('/session/ses_a/prompt_async'));
  assert.ok(prompted);
  assert.deepEqual(prompted.body, { noReply: false, parts: [{ type: 'text', text: 'act text' }] });
});

test('opencode: busy defers, a mismatched receipt or workspace is refused, credentials need their endpoint', async t => {
  const ws = workspace(t);
  const busy = await opencodeServer(t, ws, { status: 'busy' });
  const first = await transport.NativeDelivery.open('opencode', busy.endpoint, 'ses_a', ws);
  assert.equal(await first.prepare(), false);
  const wrongReceipt = await opencodeServer(t, ws, { message: { info: { sessionID: 'ses_other', role: 'user', id: 'msg_1' }, parts: [] } });
  const second = await transport.NativeDelivery.open('opencode', wrongReceipt.endpoint, 'ses_a', ws);
  await rejects(second.offer('x', 't', false), /receipt did not match submitted message\/session; inspect before retrying/, 'ValidationError');
  const elsewhere = await opencodeServer(t, ws, { directory: workspace(t) });
  const third = await transport.NativeDelivery.open('opencode', elsewhere.endpoint, 'ses_a', ws);
  await rejects(third.prepare(), /session\/workspace mismatch; no message submitted/, 'ValidationError');
  const saved = { ...process.env };
  t.after(() => { for (const key of ['OPENCODE_SERVER_PASSWORD', 'OPENCODE_SERVER_USERNAME', 'OCTOCODE_OPENCODE_AUTH_ENDPOINT']) if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key]; });
  process.env.OPENCODE_SERVER_PASSWORD = 'secret';
  await assert.rejects(transport.NativeDelivery.open('opencode', busy.endpoint, 'ses_a', ws), /OCTOCODE_OPENCODE_AUTH_ENDPOINT/);
  process.env.OCTOCODE_OPENCODE_AUTH_ENDPOINT = busy.endpoint;
  const authed = await transport.NativeDelivery.open('opencode', busy.endpoint, 'ses_a', ws);
  await authed.prepare();
  const lastCall = busy.calls.at(-1);
  assert.ok(lastCall);
  assert.equal(lastCall.authorization, 'Basic ' + Buffer.from('opencode:secret').toString('base64'));
  for (const delivery of [first, second, third, authed]) delivery.close();
});

test('opencode: a non-200 preflight is refused and a non-ses_ identifier is rejected', async t => {
  const ws = workspace(t);
  const server = createServer((_request, response) => { response.writeHead(500); response.end('no'); });
  t.after(() => { server.closeAllConnections(); server.close(); });
  await listening(server, 0, '127.0.0.1');
  const delivery = await transport.NativeDelivery.open('opencode', `http://127.0.0.1:${portOf(server)}`, 'ses_a', ws);
  t.after(() => delivery.close());
  await rejects(delivery.prepare(), /OpenCode preflight returned HTTP 500; no message submitted/);
  await assert.rejects(transport.NativeDelivery.open('opencode', 'http://127.0.0.1:1', 'not-a-session', ws), /ses_/);
});
