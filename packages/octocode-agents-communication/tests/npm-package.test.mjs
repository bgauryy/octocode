import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { readdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { connectStdio } from '../../octocode-mcp-cli/dist/index.js';
import { root, tempWorkspace } from './helpers.mjs';

test('npm archive runs through npx outside the repository with CLI and MCP communication', async t => {
  const directory = tempWorkspace(t, 'communication-npm-', { real: true });
  const options = { encoding: 'utf8', timeout: 30000, stdio: 'pipe' };
  const [packed] = JSON.parse(execFileSync('npm', [
    'pack', '--ignore-scripts', '--json', '--pack-destination', directory,
  ], { ...options, cwd: root }));
  const paths = packed.files.map(file => file.path);
  assert.ok(paths.includes('bin/octocode-agents-communication.mjs'));
  assert.ok(paths.includes('scripts/communication.py'));
  assert.ok(paths.includes('scripts/octocode_config.py'));
  assert.ok(paths.includes('OPERATING.md'));
  assert.ok(paths.includes('ARCHITECTURE.md'), 'The installed README links to the architecture guide');
  assert.ok(paths.includes('dist/cli.js'));
  assert.ok(paths.includes('dist/mcp.js'));
  assert.ok(!paths.some(path => /^(src|tests|skills)\//.test(path) || path.endsWith('.pyc') || path.includes('__pycache__')));
  const archive = join(directory, packed.filename);
  const launch = (...args) => JSON.parse(execFileSync('npx', [
    '--yes', '--offline', '--cache', join(directory, 'npm-cache'),
    '--package', archive, 'octocode-agents-communication', '/cli', ...args, '--json',
  ], { ...options, cwd: directory }));
  assert.ok(launch('--help').commands.some(command => command.name === 'join'));
  assert.ok(launch('skill').instructions.includes('## Edit with ownership'));
  assert.equal(launch('schema', 'send_message').name, 'send_message');
  const database = join(directory, 'communication.sqlite');
  const flags = ['--workspace', directory, '--database', database];
  const agent = launch('join', '{"name":"npm-check","vendor":"generic"}', ...flags);
  assert.ok(agent.id);
  assert.equal(launch('peers', ...flags).items[0].id, agent.id);
  assert.equal(launch('db', 'info', ...flags).compatible, true);
  const installs = join(directory, 'npm-cache', '_npx');
  const installed = join(installs, readdirSync(installs)[0], 'node_modules', '@octocodeai', 'octocode-agents-communication');
  const mcpModule = await import(pathToFileURL(join(installed, 'dist/mcp.js')));
  const cliModule = await import(pathToFileURL(join(installed, 'dist/cli.js')));
  assert.equal(typeof mcpModule.runCommunicationMcp, 'function');
  assert.equal(typeof cliModule.runCommunicationCli, 'function');
  const receiver = launch('join', '--name', 'mcp-receiver', '--vendor', 'generic', ...flags);
  const client = await connectStdio({ command: process.execPath, args: [
    join(installed, 'bin/octocode-agents-communication.mjs'), '--session', receiver.id, ...flags,
  ] });
  try {
    const tools = await client.listTools();
    assert.equal(tools.tools.length, 18);
    const sent = launch('send_message', '--to', receiver.id, '--body', 'CLI to packed MCP', '--reasoning', 'Verify installed interfaces', '--session', agent.id, ...flags);
    const inbox = await client.callTool({ name: 'inbox', arguments: {} });
    assert.notEqual(inbox.isError, true);
    assert.equal(JSON.parse(inbox.content[0].text).items[0].id, sent.id);
    const completed = await client.callTool({ name: 'complete', arguments: { message: sent.id, reply: 'Packed MCP to CLI' } });
    assert.notEqual(completed.isError, true);
    const reply = launch('inbox', '--session', agent.id, ...flags).items[0];
    assert.equal(reply.body, 'Packed MCP to CLI');
    assert.equal(reply.replyTo, sent.id);
    launch('complete', '--message', String(reply.id), '--session', agent.id, ...flags);
  } finally { await client.close(); }
  assert.ok(launch('peers', ...flags).items.some(peer => peer.id === receiver.id), 'Borrowed MCP preserves its identity on close');
  launch('leave', ...flags, '--session', receiver.id);
  launch('leave', ...flags, '--session', agent.id);
});

test('npm launcher reports interpreter startup failures and preserves command errors', t => {
  const directory = tempWorkspace(t, 'communication-launcher-');
  const entry = join(root, 'bin/octocode-agents-communication.mjs');
  assert.throws(() => execFileSync(process.execPath, [entry, '/cli', '--help'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe',
    env: { ...process.env, OCTOCODE_PYTHON: join(directory, 'missing-python') },
  }), error => error.status === 1 && error.stderr.includes('OCTOCODE_PYTHON'));
  assert.throws(() => execFileSync(process.execPath, [entry, 'not-a-command'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe',
  }), error => error.status !== 0);
});
