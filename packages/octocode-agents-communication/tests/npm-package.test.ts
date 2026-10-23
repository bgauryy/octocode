import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import type { ExecFileSyncOptionsWithStringEncoding } from 'node:child_process';
import { join } from 'node:path';
import { copyFileSync, mkdirSync, readdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { connectStdio } from 'octocode-mcp-cli';
import type { JsonRecord } from './json-record.ts';
import { isChildFailure, root, tempWorkspace } from './helpers.ts';

test('npm archive runs through npx outside the repository with CLI and MCP communication', async t => {
  const directory = tempWorkspace(t, 'communication-npm-', { real: true });
  const options: ExecFileSyncOptionsWithStringEncoding = { encoding: 'utf8', timeout: 30000, stdio: 'pipe' };
  const [packed] = JSON.parse(execFileSync('npm', [
    'pack', '--ignore-scripts', '--json', '--pack-destination', directory,
  ], { ...options, cwd: root }));
  const paths = packed.files.map((file: JsonRecord) => file.path);
  assert.ok(paths.includes('bin/octocode-agents-communication.mjs'));
  for (const path of ['dist/communication.js', 'dist/runtime.js', 'dist/hosts/pi-inbox.js', 'dist/hosts/pi-extension.js', 'dist/hosts/claude-lease-guard.js', 'dist/hosts/opencode-lease-guard.js', 'dist/hosts/lease-check.js', 'docs/COMMANDS.md', 'assets/catalog.json'])
    assert.ok(paths.includes(path), path);
  assert.ok(paths.includes('OPERATING.md'));
  assert.ok(paths.includes('ARCHITECTURE.md'), 'The installed README links to the architecture guide');
  assert.ok(paths.includes('dist/cli.js'));
  assert.ok(paths.includes('dist/mcp.js'));
  assert.ok(!paths.some((path: string) => /^(src|tests|skills)\//.test(path) || path.startsWith('scripts/')));
  const archive = join(directory, packed.filename);
  const launch = (...args: string[]): JsonRecord => JSON.parse(execFileSync('npx', [
    '--yes', '--offline', '--cache', join(directory, 'npm-cache'),
    '--package', archive, 'octocode-agents-communication', '/cli', ...args, '--json',
  ], { ...options, cwd: directory }));
  assert.ok(launch('--help').commands.some((command: JsonRecord) => command.name === 'join'));
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
  const mcpModule = await import(pathToFileURL(join(installed, 'dist/mcp.js')).href);
  const cliModule = await import(pathToFileURL(join(installed, 'dist/cli.js')).href);
  assert.equal(typeof mcpModule.runCommunicationMcp, 'function');
  assert.equal(typeof cliModule.runCommunicationCli, 'function');
  const receiver = launch('join', '--name', 'mcp-receiver', '--vendor', 'generic', ...flags);
  const client = await connectStdio({ command: process.execPath, args: [
    join(installed, 'bin/octocode-agents-communication.mjs'), '--session', receiver.id, ...flags,
  ], env: Object.fromEntries(Object.entries(process.env).filter((entry): entry is [string, string] => entry[1] !== undefined)) });  // the private Octocode home of helpers.ts, so the server registers its database there
  try {
    const tools = await client.listTools();
    assert.equal(tools.tools.length, 18);
    const sent = launch('send_message', '--to', receiver.id, '--question', 'CLI to packed MCP', '--reasoning', 'Verify installed interfaces', '--session', agent.id, ...flags);
    const inbox = await client.callTool({ name: 'inbox', arguments: {} });
    assert.notEqual(inbox.isError, true);
    assert.ok(Array.isArray(inbox.content));
    const [block] = inbox.content;
    assert.equal(block?.type, 'text');
    assert.equal(JSON.parse(block?.type === 'text' ? block.text : '').items[0].id, sent.id);
    const completed = await client.callTool({ name: 'complete', arguments: { message: sent.id, outcome:'done',reply:{conclusion:'Packed MCP to CLI'} } });
    assert.notEqual(completed.isError, true);
    const reply = launch('inbox', '--session', agent.id, ...flags).items[0];
    assert.equal(reply.body, 'BLUF\nOutcome: done\nConclusion: Packed MCP to CLI');
    assert.equal(reply.replyTo, sent.id);
    launch('complete', '--message', String(reply.id), '--session', agent.id, ...flags);
  } finally { await client.close(); }
  assert.ok(launch('peers', ...flags).items.some((peer: JsonRecord) => peer.id === receiver.id), 'Borrowed MCP preserves its identity on close');
  launch('leave', ...flags, '--session', receiver.id);
  launch('leave', ...flags, '--session', agent.id);
});

test('npm launcher reports startup failures and preserves command errors', t => {
  const directory = tempWorkspace(t, 'communication-launcher-');
  const entry = join(root, 'bin/octocode-agents-communication.mjs');
  const env = process.env;
  const help = execFileSync(process.execPath, [entry, '/cli', '--help'], { encoding: 'utf8', timeout: 10000, stdio: 'pipe', env });
  assert.match(help, /join/);
  // A package without its built runtime fails with the module error and a non-zero status.
  mkdirSync(join(directory, 'bin'));
  copyFileSync(entry, join(directory, 'bin/octocode-agents-communication.mjs'));
  assert.throws(() => execFileSync(process.execPath, [join(directory, 'bin/octocode-agents-communication.mjs'), '/cli', '--help'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe', env,
  }), error => isChildFailure(error) && error.status === 1 && /cli\.js/.test(error.stderr.toString()));
  assert.throws(() => execFileSync(process.execPath, [entry, 'not-a-command'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe',
  }), error => isChildFailure(error) && error.status !== 0);
});
