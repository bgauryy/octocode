// The universal message parts and contextId reach the CLI, MCP tools/list and stored messages consistently.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { binary, jsonCall, spawnSync, tempWorkspace } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

test('text, JSON and URL message parts preserve order and context across MCP and CLI schemas', t => {
  const workspace = tempWorkspace(t, 'communication-message-parts-', { real: true }), database = join(workspace, 'parts.sqlite');
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const alice = call('join', { name: 'alice', vendor: 'generic' }).id, bob = call('join', { name: 'bob', vendor: 'generic' }).id;
  const parts = [
    { text: 'First' },
    { data: { phase: 'verify', opaque: { constraints: ['read only'], handoff: true } }, mediaType: 'application/json', filename: 'phase.json' },
    { url: 'https://example.invalid/evidence', mediaType: 'text/plain', filename: 'evidence.txt' },
  ];
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'messages', arguments: { type: 'send', input: { to: bob, parts, contextId: 'review:api', replyRequired: true, reasoning: 'Hand off evidence' } } } },
  ].map(frame => JSON.stringify(frame)).join('\n') + '\n';
  const run = spawnSync(binary, ['mcp', '--tools', 'messages', '--session', alice, '--workspace', workspace, '--database', database], { input: frames, encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  const rows = run.stdout.trim().split('\n').map((line): JsonRecord => JSON.parse(line));
  const family = rows[0].result.tools.find((tool: JsonRecord) => tool.name === 'messages').inputSchema;
  const cli = JSON.parse(spawnSync(binary, ['schema', 'tools', '--tools', 'messages', '--workspace', workspace, '--database', database], { encoding: 'utf8' }).stdout).find((tool: JsonRecord) => tool.name === 'messages');
  assert.deepEqual(cli.inputSchema, family, 'CLI schema and MCP tools/list describe the same family contract');
  assert.equal(rows[1].result.isError, undefined);
  const stored = JSON.parse(spawnSync(binary, ['inbox', '{}', '--workspace', workspace, '--database', database, '--session', bob], { encoding: 'utf8' }).stdout).items[0];
  assert.deepEqual(stored.parts, parts);
  assert.equal(stored.contextId, 'review:api');
  assert.equal(stored.replyRequired, true);
});
