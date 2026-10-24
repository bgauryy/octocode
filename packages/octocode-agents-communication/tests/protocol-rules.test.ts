// The message-protocol cross-reference rules reach the public CLI and MCP server unchanged; store-protocol-rules.test.ts
// and store-protocol.test.ts own the rules themselves.
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { answer, binary, decision, execFileSync, jsonCall, restate, task, tempWorkspace, withReasoning } from './helpers.ts';
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
    const out = execFileSync(binary, ['mcp', '--tools', 'send_message,complete', '--workspace', workspace, '--database', database, '--session', session], { input: frames, encoding: 'utf8', timeout: 15000 });
    return out.trim().split('\n').map(line => JSON.parse(line));
  };
  return { call, alice, bob, carol, inbox, mcp };
}

// --- MCP parity ------------------------------------------------------------------------------------------------

test('Given the MCP server, when cross-reference rules are broken, then the same errors surface and valid calls produce the same bodies', t => {
  const f = fixture(t);
  const asked = f.call('send_message', { to: 'bob', task: task({ checkBack: true, informed: ['carol'] }) }, f.alice);
  const text = (row: JsonRecord) => row.result.content[0].text;
  const rows = f.mcp(f.bob, [
    { name: 'send_message', arguments: { to: 'alice', decision: decision({ default: 'mysql' }), reasoning: 'Verify the decision default rule over MCP' } },
    { name: 'complete', arguments: { ...answer(asked.id, 'Fine'), reasoning: 'Verify the check-back gate over MCP' } },
    { name: 'send_message', arguments: { to: 'alice', checkback: restate(asked.id), reasoning: 'Verify the check-back is accepted over MCP' } },
    { name: 'complete', arguments: { ...answer(asked.id, 'Fine'), reasoning: 'Verify completion after check-back over MCP' } },
  ]);
  assert.equal(rows[0].result.isError, true);
  assert.match(text(rows[0]), /Decision default must be one of its options/);
  assert.equal(rows[1].result.isError, true);
  assert.match(text(rows[1]), /asks for a check-back/);
  assert.equal(JSON.parse(text(rows[2])).recipients, 1);
  assert.equal(JSON.parse(text(rows[3])).completed, true);
  assert.equal(f.inbox(f.carol)[0].body, `BLUF\nRe task ${asked.id}: Review storage\nOutcome: done\nConclusion: Fine`);
});
