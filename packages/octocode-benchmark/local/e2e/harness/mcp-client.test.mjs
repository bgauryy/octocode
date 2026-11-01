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
