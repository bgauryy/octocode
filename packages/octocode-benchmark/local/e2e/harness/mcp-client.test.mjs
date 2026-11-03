import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { spawnCli, startServer } from './mcp-client.mjs';

const text = 'before λ🦀 after';
const splitWriter = `
const text = ${JSON.stringify(text)};
async function split(stream, value) {
  const bytes = Buffer.from(value);
  const at = bytes.indexOf(Buffer.from('🦀')) + 1;
  stream.write(bytes.subarray(0, at));
  await new Promise(resolve => setTimeout(resolve, 25));
  await new Promise(resolve => stream.write(bytes.subarray(at), resolve));
}
`;

test('CLI stdout and stderr preserve UTF-8 characters split across pipe chunks', async t => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-cli-utf8-'));
  t.after(() => fs.rm(dir, { recursive: true, force: true }));
  const bin = path.join(dir, 'writer.mjs');
  await fs.writeFile(bin, splitWriter + '\nawait split(process.stdout, text); await split(process.stderr, text);');
  const result = await spawnCli([], { bin, timeoutMs: 10_000 });
  assert.equal(result.status, 0);
  assert.equal(result.stdout, text);
  assert.equal(result.stderr, text);
});

test('MCP JSON and stderr preserve UTF-8 characters split across pipe chunks', async t => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-mcp-utf8-'));
  t.after(() => fs.rm(dir, { recursive: true, force: true }));
  const entry = path.join(dir, 'server.mjs');
  await fs.writeFile(entry, splitWriter + `
import readline from 'node:readline';
for await (const line of readline.createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  const result = request.method === 'tools/list' ? { tools: [] }
    : request.method === 'tools/call' ? { content: [{ type: 'text', text }], structuredContent: { value: text } }
    : { protocolVersion: '2025-06-18', capabilities: {}, serverInfo: { name: 'fixture', version: '1' } };
  const frame = JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\\n';
  if (request.method === 'tools/call') {
    await split(process.stderr, text);
    await split(process.stdout, frame);
  } else process.stdout.write(frame);
}
`);
  const server = await startServer({ entry, timeoutMs: 10_000 });
  t.after(() => server.close());
  const result = await server.raw('fixture', {});
  assert.equal(result.isError, false);
  assert.equal(result.text, text);
  assert.deepEqual(result.sc, { value: text });
  assert.equal(server.stderr(), text);
});

const firstPage = { results: [{ index: 0, data: { content: text } }, { index: 1, status: 'error', data: { errorCode: 'notFound', error: 'missing' } }],
  responsePagination: { page: 1, totalPages: 2, hasMore: true, next: { kind: 'continue', tool: 'fixture', input: { cursor: 'emitted-cursor' } } } };
const lastPage = { results: [{ index: 0, data: { content: 'remaining evidence' } }], responsePagination: { page: 2, totalPages: 2, hasMore: false } };
const rejected = { error: 'Invalid query', errorCode: 'invalidInput' };

async function jsonTextServer(t) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-mcp-json-text-'));
  t.after(() => fs.rm(dir, { recursive: true, force: true }));
  const entry = path.join(dir, 'server.mjs');
  await fs.writeFile(entry, splitWriter + `
import readline from 'node:readline';
for await (const line of readline.createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  let result;
  if (request.method === 'tools/list') result = { tools: [] };
  else if (request.method !== 'tools/call') result = { protocolVersion: '2025-06-18', capabilities: {}, serverInfo: { name: 'fixture', version: '1' } };
  else {
    const tool = request.params.name;
    if (tool === 'protocolError') {
      process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: { code: -32602, message: 'Unknown tool' } }) + '\\n');
      continue;
    }
    const payload = tool === 'rejected' ? ${JSON.stringify(rejected)}
      : request.params.arguments.cursor ? ${JSON.stringify(lastPage)} : ${JSON.stringify(firstPage)};
    result = { content: [{ type: 'text', text: tool === 'malformed' ? '{invalid JSON' : JSON.stringify(payload) }], isError: tool === 'rejected', _meta: { source: 'fixture' } };
  }
  await split(process.stdout, JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\\n');
}
`);
  const server = await startServer({ entry, timeoutMs: 10_000 });
  t.after(() => server.close());
  return server;
}

test('JSON-text MCP responses normalize every row and continuation while retaining raw wire evidence', async t => {
  const server = await jsonTextServer(t);
  let measured;
  const first = await server.raw('fixture', { queries: [{}] }, 'first', { keepRaw: true, bytes: (data, shown) => {
    measured = { data, shown };
    return Buffer.byteLength(shown, 'utf8');
  } });
  assert.deepEqual(first.sc, firstPage);
  assert.deepEqual(first.raw, firstPage);
  assert.equal(first.rowErrors, 1);
  assert.equal(first.isError, false, 'a mixed result remains a successful tool call');
  assert.deepEqual(measured, { data: firstPage, shown: JSON.stringify(firstPage) });
  assert.deepEqual(first.wire.result, { content: [{ type: 'text', text: JSON.stringify(firstPage) }], isError: false, _meta: { source: 'fixture' } });
  assert.equal(first.responseBytes, Buffer.byteLength(JSON.stringify(first.wire.result), 'utf8'));
  const last = await server.follow(first.sc.responsePagination.next, 'last');
  assert.deepEqual(last.args, firstPage.responsePagination.next.input);
  assert.deepEqual(last.sc, lastPage);
});

test('JSON-text tool errors preserve the typed error envelope and error flag', async t => {
  const server = await jsonTextServer(t);
  const result = await server.raw('rejected', {}, '', { keepRaw: true });
  assert.deepEqual(result.sc, rejected);
  assert.deepEqual(result.raw, rejected);
  assert.equal(result.isError, true);
  assert.equal(result.wire.result.isError, true);
});

test('invalid JSON text and protocol errors remain explicit failures with their raw evidence', async t => {
  const server = await jsonTextServer(t);
  const malformed = await server.raw('malformed', {}, '', { keepRaw: true });
  assert.equal(malformed.isError, true);
  assert.equal(malformed.sc, undefined);
  assert.equal(malformed.text, '{invalid JSON');
  assert.equal(malformed.wire.result.content[0].text, malformed.text);
  const rejectedCall = await server.raw('protocolError', {}, '', { keepRaw: true });
  assert.equal(rejectedCall.isError, true);
  assert.equal(rejectedCall.sc, undefined);
  assert.deepEqual(rejectedCall.wire.error, { code: -32602, message: 'Unknown tool' });
  assert.equal(rejectedCall.transport, 'Unknown tool');
});
