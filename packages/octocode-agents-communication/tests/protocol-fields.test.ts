// handoff.constraints and delta.phase reach every interface: the CLI, MCP tools/list and MCP tools/call agree.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { binary, jsonCall, spawnSync, tempWorkspace, withReasoning } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';


test('Given the handoff constraints and delta phase slots, when they are sent over MCP, then discovery, CLI schema and the stored bodies agree', t => {
  const workspace = tempWorkspace(t, 'communication-fields-', { real: true }), database = join(workspace, 'fields.sqlite');
  const raw = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const call = (command: string, input: Record<string, unknown>, session?: string) => raw(command, withReasoning(command, input), session);
  const alice = call('join', { name: 'alice', vendor: 'generic' }).id, bob = call('join', { name: 'bob', vendor: 'generic' }).id;
  const handoff = { state: 'Half done', next: 'Finish it', constraints: ['read only'] };
  const delta = { phase: 'verify', changes: ['tests pass'] };
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'send_message', arguments: { to: bob, handoff, reasoning: 'Hand the work over' } } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'send_message', arguments: { to: bob, delta, reasoning: 'Report the current phase' } } },
    { jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'send_message', arguments: { to: bob, delta: { ...delta, phase: ' ' }, reasoning: 'A blank phase is refused' } } },
  ].map(frame => JSON.stringify(frame)).join('\n') + '\n';
  const run = spawnSync(binary, ['mcp', '--session', alice, '--workspace', workspace, '--database', database], { input: frames, encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  const rows = run.stdout.trim().split('\n').map((line): JsonRecord => JSON.parse(line));
  const properties = rows[0].result.tools.find((tool: JsonRecord) => tool.name === 'send_message').inputSchema.properties;
  assert.deepEqual(properties.handoff.properties.constraints.items, { type: 'string' });
  assert.deepEqual(Object.keys(properties.handoff.properties), ['state', 'evidence', 'blockers', 'next', 'constraints', 'leases'], 'schema order is protocol order');
  assert.deepEqual(Object.keys(properties.delta.properties), ['phase', 'changes', 'blockers', 'next']);
  const cli = JSON.parse(spawnSync(binary, ['schema', '--workspace', workspace, '--database', database], { encoding: 'utf8' }).stdout).tools.find((tool: JsonRecord) => tool.name === 'send_message');
  assert.deepEqual(cli.inputSchema.properties, properties, 'CLI schema and MCP tools/list describe the same fields');
  assert.equal(rows[1].result.isError, undefined);
  assert.equal(rows[2].result.isError, undefined);
  assert.equal(rows[3].result.isError, true);
  assert.match(rows[3].result.content[0].text, /blank/);
  // store-protocol.test.ts owns the rendered layout; here the slots only have to arrive through MCP.
  const bodies: string[] = call('inbox', {}, bob).items.map((item: JsonRecord) => item.body);
  assert.equal(bodies.length, 2, 'the refused blank phase stored nothing');
  const [stored, phased] = bodies;
  assert.match(stored, /\nConstraints:\n- read only$/);
  assert.match(phased, /\nPhase: verify\n/);
});
