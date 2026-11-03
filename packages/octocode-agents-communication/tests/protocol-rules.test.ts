// The universal multipart contract reaches the public CLI and MCP server unchanged; store-protocol-rules.test.ts
// checks opaque payloads do not trigger policy side effects.
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { binary, execFileSync, jsonCall, tempWorkspace, withReasoning } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-protocol-rules-', { real: true }), database = join(workspace, 'rules.sqlite');
  const raw = jsonCall(binary, workspace, database, { stdio: 'pipe', timeout: 15000 });
  const call = (command: string, input: Record<string, unknown>, session?: string) => raw(command, withReasoning(command, input), session);
  const join_ = (name: string) => call('join', { name, vendor: 'generic' }).id;
  const alice = join_('alice'), bob = join_('bob'), carol = join_('carol');
  const inbox = (session: string) => call('inbox', {}, session).items;
  const mcp = (session: string, calls: JsonRecord[]) => {
    const frames = calls.map((params, index) => JSON.stringify({ jsonrpc: '2.0', id: index + 1, method: 'tools/call', params })).join('\n') + '\n';
    const out = execFileSync(binary, ['mcp', '--tools', 'messages', '--workspace', workspace, '--database', database, '--session', session], { input: frames, encoding: 'utf8', timeout: 15000 });
    return out.trim().split('\n').map(line => JSON.parse(line));
  };
  return { call, alice, bob, carol, inbox, mcp };
}

test('MCP sends opaque structured parts and completes the exact correlated request', t => {
  const f = fixture(t);
  const opaque = { question: 'metadata only', informed: ['carol'], checkBack: true, default: 'invalid', leases: [999], evidence: [{ source: 'missing.md' }] };
  const request = f.call('send_message', { to: f.bob, parts: [{ data: opaque }], replyRequired: true, contextId: 'mcp-context' }, f.alice);
  const text = (row: JsonRecord) => row.result.content[0].text;
  const rows = f.mcp(f.bob, [
    { name: 'messages', arguments: { type: 'complete', input: { message: request.id, outcome: 'completed', reply: [{ text: 'Reviewed over MCP' }] } } },
  ]);
  assert.equal(rows[0].result.isError, undefined);
  assert.deepEqual(JSON.parse(text(rows[0])), { completed: true, count: 1, id: JSON.parse(text(rows[0])).id, recipients: 1 });
  const reply = f.inbox(f.alice).find((item: JsonRecord) => item.replyTo === request.id);
  assert.ok(reply);
  assert.deepEqual(reply.parts, [{ text: 'Reviewed over MCP' }]);
  assert.equal(reply.contextId, 'mcp-context');
  assert.equal(f.inbox(f.carol).length, 0, 'informed-shaped data does not route copies');
  assert.deepEqual(f.inbox(f.bob), []);
});

test('MCP family schemas reject removed message-kind fields and accept explicit typed parts', t => {
  const f = fixture(t);
  const rows = f.mcp(f.alice, [
    { name: 'messages', arguments: { type: 'send', input: { to: f.bob, parts: [{ text: 'valid' }], reasoning: 'Validate the public multipart contract' } } },
    { name: 'messages', arguments: { type: 'send', input: { to: f.bob, question: 'removed field' } } },
  ]);
  assert.equal(rows[0].result.isError, undefined);
  assert.equal(rows[1].result.isError, true);
  assert.equal(f.inbox(f.bob).length, 1);
});
