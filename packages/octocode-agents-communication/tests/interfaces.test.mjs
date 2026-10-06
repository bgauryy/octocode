import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { createCommunicationCli, runCommunicationCli } from '../dist/cli.js';
import { commandHelp, parseCommandInput } from '../../octocode-mcp-cli/dist/index.js';
import { root, tempWorkspace } from './helpers.mjs';
const entry = join(root, 'bin/octocode-agents-communication.mjs');
const launch = args => JSON.parse(execFileSync(process.execPath, [entry, '/cli', ...args, '--json'], { encoding: 'utf8', timeout: 10000 }));
test('all canonical operations expose contextual help with no unexplained flags', () => {
  const spec = createCommunicationCli();
  assert.equal(spec.commands.length, 52);
  for (const command of spec.commands) {
    assert.ok(commandHelp(spec, command).includes(command.description), command.name);
    for (const flag of command.flags) assert.ok(flag.description, `${command.name}.${flag.property}`);
  }
  const send = spec.commands.find(c => c.name === 'send_message');
  assert.throws(() => parseCommandInput(send, { body: 'x', reasoning: 'x', session: 'id' }));
  assert.throws(() => parseCommandInput(send, { to: 'peer', topic: 'topic', body: 'x', reasoning: 'x', session: 'id' }));
  assert.match(commandHelp(spec, send), /EXAMPLES[\s\S]*--to/);
  const complete = spec.commands.find(c => c.name === 'complete');
  assert.match(commandHelp(spec, complete), /EXAMPLES[\s\S]*--message 1/);
});
test('CLI preserves identities and leases across invocations; default MCP borrows them', t => {
  const workspace = tempWorkspace(t, 'communication-interfaces-', { real: true });
  const common = ['--workspace-root', workspace, '--database', join(workspace, 'db.sqlite')];
  const identity = launch(['join', '--name', 'reviewer', '--vendor', 'generic', ...common]);
  const bound = ['--session', identity.id, ...common];
  const lease = launch(['lock', '--path', 'owned.txt', '--reasoning', 'Protect review edits', ...bound]);
  assert.equal(lease.ok, true);
  assert.equal(launch(['locks', ...bound]).items.length, 1);
  const input = [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'test', version: '1' } } },
    { jsonrpc: '2.0', id: 2, method: 'tools/list' },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'locks', arguments: {} } },
  ].map(JSON.stringify).join('\n') + '\n';
  const output = execFileSync(process.execPath, [entry, '--session', identity.id, '--workspace', workspace, '--database', join(workspace, 'db.sqlite')], { input, encoding: 'utf8', timeout: 10000 }).trim().split('\n').map(JSON.parse);
  assert.ok(output.find(row => row.id === 2).result.tools.some(tool => tool.name === 'send_message'));
  assert.equal(output.find(row => row.id === 3).result.isError, undefined);
  assert.equal(launch(['locks', ...bound]).items.length, 1);
  assert.equal(launch(['db-info', ...common]).compatible, true);
  assert.ok(launch(['db-protocol']).protocol);
  assert.ok(launch(['schema', 'types', '--compact']).length > 0);
  assert.equal(launch(['schema', 'type', 'coordinate.in']).type, 'coordinate.in');
  assert.equal(launch(['schema', 'send_message']).name, 'send_message');
  const large = JSON.parse(execFileSync(process.execPath, [entry, '/cli', 'share_document', '-', ...bound, '--json'], {
    input: JSON.stringify({ name: 'large-evidence', content: 'e'.repeat(300000), reasoning: 'Verify lossless stdin transport' }), encoding: 'utf8', timeout: 10000,
  }));
  assert.ok(large);
  let page = launch(['read_document', '--name', 'large-evidence', '--limit', '16384', ...bound]);
  let content = page.content;
  while (page.next) { page = launch([page.next.command, JSON.stringify(page.next.input), ...bound]); content += page.content; }
  assert.equal(content, 'e'.repeat(300000));
  launch(['subscribe', '{"topics":[]}', ...bound]);
  assert.throws(() => launch(['fetch', '{"limit":"10"}', ...bound]));
  // JSON-valued flags are values, never compatibility payloads.
  assert.ok(launch(['fetch', '--where', '{"name":"reviewer"}', ...bound]).items);
  launch(['leave', ...bound]);
});
test('streaming CLI emits no extra result frame after MCP EOF', t => {
  const workspace = tempWorkspace(t, 'communication-stream-', { real: true });
  const output = execFileSync(process.execPath, [entry, '/cli', 'mcp', '--managed', '--vendor', 'generic', '--name', 'stream', '--workspace-root', workspace, '--database', join(workspace, 'db.sqlite'), '--json'], { input: '', encoding: 'utf8', timeout: 10000 });
  assert.equal(output, '');
});
