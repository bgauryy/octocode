import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { handleMcpAction, stopAllMcpServers } from '../src/tools/mcp-tool.js';
import { projectMcpPath } from '../src/tools/mcp-config.js';
import type { PiContext, ToolCallResult } from '../src/types.js';

const roots: string[] = [];
const servers: http.Server[] = [];

afterEach(async () => {
  stopAllMcpServers();
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve) => server.close(() => resolve()))));
  while (roots.length) fs.rmSync(roots.pop()!, { recursive: true, force: true });
});

function text(result: ToolCallResult): string {
  return (result.content[0] as { text?: string } | undefined)?.text ?? '';
}

async function fixtureServer(): Promise<{ url: string; methods: string[] }> {
  const methods: string[] = [];
  const server = http.createServer((request, response) => {
    void (async () => {
      if (request.method === 'DELETE') {
        response.writeHead(204).end();
        return;
      }
      const chunks: Buffer[] = [];
      for await (const chunk of request) chunks.push(Buffer.from(chunk));
      if (chunks.length === 0) {
        response.writeHead(204).end();
        return;
      }
      const message = JSON.parse(Buffer.concat(chunks).toString('utf8')) as { id?: string | number; method: string; params?: Record<string, unknown> };
      methods.push(message.method);
      if (message.id === undefined) {
        response.writeHead(202).end();
        return;
      }
      let result: unknown;
      switch (message.method) {
        case 'initialize':
          result = {
            protocolVersion: String(message.params?.['protocolVersion'] ?? '2025-11-25'),
            capabilities: { tools: {}, resources: {}, prompts: {}, completions: {} },
            serverInfo: { name: 'http-fixture', version: '1.0.0' },
            instructions: 'Use the fixture exactly.',
          };
          break;
        case 'tools/list':
          result = { tools: [{ name: 'echo', description: 'Echo required text.', inputSchema: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } }] };
          break;
        case 'tools/call':
          result = { content: [{ type: 'text', text: `echo:${String((message.params?.['arguments'] as Record<string, unknown>)?.['text'])}` }] };
          break;
        case 'resources/list':
          result = { resources: [{ uri: 'fixture://readme', name: 'readme' }] };
          break;
        case 'resources/templates/list':
          result = { resourceTemplates: [{ uriTemplate: 'fixture://{name}', name: 'fixture-template' }] };
          break;
        case 'resources/read':
          result = { contents: [{ uri: String(message.params?.['uri']), mimeType: 'text/plain', text: 'fixture resource' }] };
          break;
        case 'prompts/list':
          result = { prompts: [{ name: 'review', description: 'Review input.', arguments: [{ name: 'topic', required: true }] }] };
          break;
        case 'prompts/get':
          result = { description: 'Review prompt', messages: [{ role: 'user', content: { type: 'text', text: `Review ${String((message.params?.['arguments'] as Record<string, unknown>)?.['topic'])}` } }] };
          break;
        case 'completion/complete':
          result = { completion: { values: ['alpha', 'beta'], total: 2, hasMore: false } };
          break;
        default:
          response.writeHead(200, { 'content-type': 'application/json', 'mcp-session-id': 'fixture-session' });
          response.end(JSON.stringify({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'Unknown method' } }));
          return;
      }
      response.writeHead(200, { 'content-type': 'application/json', 'mcp-session-id': 'fixture-session' });
      response.end(JSON.stringify({ jsonrpc: '2.0', id: message.id, result }));
    })();
  });
  servers.push(server);
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  return { url: `http://127.0.0.1:${address.port}/mcp`, methods };
}

test('Streamable HTTP supports tools, resources, templates, prompts, reads, completion, and calls', async () => {
  const fixture = await fixtureServer();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-mcp-http-'));
  roots.push(root);
  process.env.OCTOCODE_HOME = path.join(root, 'octocode-home');
  const configPath = projectMcpPath(root);
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, JSON.stringify({ mcpServers: { remote: { url: fixture.url, timeoutMs: 5_000 } } }));
  const ctx = { cwd: root, isProjectTrusted: async () => true } as unknown as PiContext;

  assert.match(text(await handleMcpAction({ action: 'describe', server: 'remote', tool: 'echo' }, undefined, ctx)), /required.*text|text.*required/i);
  assert.match(text(await handleMcpAction({ action: 'resources', server: 'remote' }, undefined, ctx)), /fixture:\/\/readme/);
  assert.match(text(await handleMcpAction({ action: 'read-resource', server: 'remote', uri: 'fixture://readme' }, undefined, ctx)), /fixture resource/);
  assert.match(text(await handleMcpAction({ action: 'prompts', server: 'remote' }, undefined, ctx)), /review/);
  assert.match(text(await handleMcpAction({ action: 'get-prompt', server: 'remote', name: 'review', arguments: { topic: 'MCP' } }, undefined, ctx)), /Review MCP/);
  assert.match(text(await handleMcpAction({ action: 'complete', server: 'remote', ref: { type: 'ref/prompt', name: 'review' }, argument: { name: 'topic', value: 'a' } }, undefined, ctx)), /alpha/);
  assert.match(text(await handleMcpAction({ action: 'call', server: 'remote', tool: 'echo', arguments: { text: 'hello' } }, undefined, ctx)), /echo:hello/);
  for (const method of ['initialize', 'tools/list', 'resources/list', 'resources/templates/list', 'resources/read', 'prompts/list', 'prompts/get', 'completion/complete', 'tools/call']) {
    assert.ok(fixture.methods.includes(method), `${method} reached the HTTP server`);
  }
});
