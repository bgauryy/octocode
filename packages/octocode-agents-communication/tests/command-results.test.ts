// Every command's real result against the `results` schema of the catalog, through Store.call, the CLI and the streaming entries.
import assert from 'node:assert/strict';
import { execFileSync as nodeExecFileSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import { DatabaseSync } from 'node:sqlite';
import { binary, commandArgs, execFileSync, spawn, tempWorkspace, waitFor } from './helpers.ts';
import { fixture, runtime } from './store-support.ts';
import type { Json } from '../src/runtime/validation.ts';

const { catalog, cli, validation } = runtime;
const results = catalog.catalog().results as Record<string, Json>;
const commands = catalog.catalog().commands.map((item: { name: string }) => item.name);
const covered = new Set<string>();

/** Asserts that `value`, as printed on the wire, satisfies the result schema of `name`. */
function conforms(name: string, value: unknown): void {
  const schema = results[name];
  assert.ok(schema !== undefined, `${name} has a result schema`);
  try {
    validation.validate(validation.asObject(schema, `${name} result schema`), value);
  } catch (error) {
    throw new Error(`${name} result violates its schema: ${(error as Error).message}\n${JSON.stringify(value).slice(0, 600)}`, { cause: error });
  }
  covered.add(name);
}

test('every command has a result schema and every result schema names a command', () => {
  assert.deepEqual(Object.keys(results).sort(), [...commands].sort());
  for (const name of commands) assert.doesNotThrow(() => validation.checkSchema(results[name]), name);
});

type Call = (name: string, input: Record<string, unknown> | null, session?: string, flags?: readonly string[]) => Promise<Record<string, unknown>>;

interface Driver {
  call: Call;
  workspace: string;
  file: string;
}

/** The CLI: argv through `cli.capture`, which prints the wire JSON. A null input runs a command that takes none. */
function cliCall(workspace: string, file: string): Call {
  const args = commandArgs(workspace, file);
  return async (name, input, session, flags = []) => {
    const argv = input === null ? [...name.split(' '), '--workspace', workspace, '--database', file, ...flags] : [...name.split(' '), ...args(name, input, session).slice(1), ...flags];
    return await cli.capture(argv) as Record<string, unknown>;
  };
}

function cliDriver(t: TestContext): Driver {
  const workspace = tempWorkspace(t, 'results-cli-', { real: true });
  nodeExecFileSync('git', ['init', '-q', '-b', 'main', workspace]);
  const file = join(workspace, 'results.sqlite');
  return { call: cliCall(workspace, file), workspace, file };
}

/** Store.call for the commands the store runs, the CLI for the rest (delivery loops, hooks, database tools). */
function storeDriver(t: TestContext): Driver {
  const f = fixture(t, { git: true });
  const fallback = cliCall(f.workspace, f.file);
  const call: Call = async (name, input, session, flags) => {
    if (input === null) return fallback(name, input, session, flags);
    try {
      return await f.call(session ?? '', name, input);
    } catch (error) {
      if (error instanceof Error && /^Unknown operation: /.test(error.message)) return fallback(name, input, session, flags);
      throw error;
    }
  };
  return { call, workspace: f.workspace, file: f.file };
}

const REASON = 'Verify the result schemas in this isolated fixture';

/** One scenario through every command a driver can run; each result is checked and recorded. */
async function scenario(driver: Driver, { exhaustive }: { exhaustive: boolean }): Promise<void> {
  const ok = async (name: string, input: Record<string, unknown> | null, session?: string, flags?: readonly string[]) => {
    const value = await driver.call(name, input, session, flags);
    conforms(name, value);
    return value;
  };
  const workspaceFile = (path: string, content: string) => { mkdirSync(join(driver.workspace, 'src'), { recursive: true }); writeFileSync(join(driver.workspace, path), content); };
  workspaceFile('a.txt', 'x');
  const alice = (await ok('join', { name: 'alice', vendor: 'claude', branch: 'main' })).id as string;
  const bob = (await ok('join', { name: 'bob', vendor: 'codex' })).id as string;
  await ok('binding', {}, alice);
  await ok('heartbeat', { renewLeases: true }, alice);
  await ok('heartbeat', {}, bob);
  await ok('set_status', { task: 'checking results', status: 'busy' }, alice);
  await ok('peers', {}, alice);
  await ok('subscribe', { topics: ['api'] }, bob);
  const asked = await ok('send_message', { to: bob, question: 'Is the schema right?', reasoning: REASON }, alice);
  await ok('send_message', { topic: 'api', body: 'fyi', reasoning: REASON, key: 'results-1' }, alice);
  await ok('notify_all', { body: 'everyone', reasoning: REASON }, alice);
  await ok('inbox', {}, bob);
  await ok('inbox', { message: asked.id }, bob);
  await ok('inbox wait', { timeoutMs: 1000 }, bob);
  await ok('fetch', { type: 'message' }, alice);
  await ok('fetch', { incoming: true }, bob);
  await ok('record', { type: 'memory', data: { content: 'remember', tags: ['a'] } }, alice);
  await ok('record', { type: 'checkpoint', data: { goal: 'g', done: [], inProgress: [], next: 'n', files: [] } }, alice);
  await ok('record', { type: 'event', data: { name: 'results.checked' }, key: 'event-1', to: bob }, alice);
  await ok('binding', {}, alice);
  await ok('lock', { path: 'a.txt', reasoning: REASON }, alice);
  const tree = await ok('lock', { path: 'src', kind: 'tree', reasoning: REASON }, alice);
  await ok('lock', { path: 'a.txt', reasoning: REASON }, bob);
  await ok('lock', { path: 'a.txt', reasoning: REASON, wait: true }, bob);
  await ok('lock', { path: 'a.txt', reasoning: REASON }, alice);
  await ok('lock_many', { paths: [{ path: 'q1' }, { path: 'q2' }], reasoning: REASON }, bob);
  await ok('lock_many', { paths: [{ path: 'src/x' }], reasoning: REASON }, bob);
  await ok('locks', {}, alice);
  await ok('locks', { presence: 'all', path: 'src' }, alice);
  await ok('check_paths', { paths: [{ path: 'src', kind: 'tree' }] }, bob);
  await ok('check_write', { paths: [{ path: 'a.txt', kind: 'file' }, { path: 'zz.txt' }] }, alice);
  const lease = (tree.lease as { id: number }).id;
  await ok('renew', { leaseId: lease }, alice);
  await ok('renew', { leaseId: 99999 }, alice);
  const document = { name: 'd.md', content: 'hello', reasoning: REASON, context: { summary: 'a note', path: 'src' } };
  await ok('share_document', document, alice);
  await ok('share_document', document, alice);
  await ok('read_document', { name: 'd.md' }, bob);
  await ok('context', { path: 'src/a.ts' }, bob);
  await ok('attach', { transport: 'raw' }, bob);
  const task = await ok('send_message', { to: bob, task: { objective: 'Do it', inputs: ['a'], output: 'o', done: 'Given x, when y, then z' }, reasoning: REASON }, alice);
  const offered = await ok('hook', { format: 'json', deferConfirm: true }, bob);
  const items = offered.items as { id: number; dispatchToken: string }[];
  await ok('confirm_delivery', { items: items.map(({ id, dispatchToken }) => ({ id, dispatchToken })) }, bob);
  const fyi = await ok('send_message', { to: bob, body: 'one more', reasoning: REASON }, alice);
  await ok('hook', { format: 'claude' }, bob);
  const staged = await ok('send_message', { to: bob, question: 'and another?', reasoning: REASON }, alice);
  await ok('hook', { format: 'json', deferConfirm: true }, bob);
  await ok('retry_delivery', { message: staged.id, reason: 'Receiver inspection confirms retry is needed' }, bob);
  await ok('record_usage', { key: 'usage-1', scope: 'turn', model: 'm', inputTokens: 1, outputTokens: 2 }, bob);
  await ok('dispatch', {}, bob);
  await ok('health', {});
  await ok('health', { limit: 1 });
  await ok('complete', { message: asked.id, outcome: 'done', reply: { conclusion: 'It is right' } }, bob);
  await ok('complete', { message: task.id, outcome: 'done', reply: { conclusion: 'Done' } }, bob);
  await ok('complete', { messages: [fyi.id] }, bob);
  await ok('activity', { view: 'files', limit: 5 }, alice);
  await ok('activity', { view: 'commits' }, alice);
  await ok('activity', { view: 'reflog' }, alice);
  await ok('prune', {}, alice);
  await ok('unlock', { leaseId: lease }, alice);
  await ok('unlock', { leaseId: lease }, alice);
  // A session of the same vendor that expired is revived with its leases.
  const db = new DatabaseSync(driver.file);
  try { db.prepare('UPDATE sessions SET expiresAt=1 WHERE id=?').run(alice); } finally { db.close(); }
  await ok('resume', { vendor: 'claude' }, alice);
  if (!exhaustive) return;
  await ok('db info', null);
  await ok('db protocol', null);
  await ok('db retention', {});
  await ok('db compact', {});
  await ok('db export', { path: join(driver.workspace, 'export.sqlite') });
  await ok('skill', null);
  await ok('schema', null);
  await ok('schema tools', null);
  for (const vendor of ['claude', 'codex', 'cursor', 'grok']) await ok('host-config', null, undefined, ['--vendor', vendor]);
  await ok('leave', {}, alice);
}

test('CLI results satisfy their result schemas', async t => {
  await scenario(cliDriver(t), { exhaustive: true });
});

test('Store.call results satisfy their result schemas', async t => {
  await scenario(storeDriver(t), { exhaustive: false });
});

/** The JSON lines a child prints, parsed. */
const lines = (text: string): unknown[] => text.split('\n').filter(Boolean).map((line): unknown => JSON.parse(line));

/** A started streaming command: its stdout lines so far, and how to stop it. */
function streaming(t: TestContext, args: readonly string[], env: NodeJS.ProcessEnv = process.env) {
  const child = spawn(binary, args, { env });
  let stdout = '', stderr = '';
  child.stdout.on('data', chunk => { stdout += chunk; });
  child.stderr.on('data', chunk => { stderr += chunk; });
  const closed = new Promise<number | null>(resolve => child.once('close', code => resolve(code)));
  t.after(async () => { if (child.exitCode === null) child.kill('SIGKILL'); await closed; });
  const output = (): unknown[] => lines(stdout.slice(0, stdout.lastIndexOf('\n') + 1));
  const stop = async () => { child.kill('SIGINT'); await closed; };
  return { output, stop, closed, stderr: () => stderr };
}

test('completion-check results satisfy their schema', async t => {
  const f = fixture(t, { git: true });
  const call = cliCall(f.workspace, f.file);
  const sender = (await call('join', { name: 'sender', vendor: 'generic' })).id as string;
  const recipient = (await call('join', { name: 'recipient', vendor: 'claude' })).id as string;
  const claude = { transport: 'claude', endpoint: join(f.workspace, 'unused.sock'), vendorSession: 'native-recipient' };
  await call('attach', claude, recipient);
  const event = { hook_event_name: 'Stop', stop_hook_active: false, session_id: 'native-recipient', cwd: f.workspace };
  conforms('completion-check', await call('completion-check', event, recipient));
  await call('send_message', { to: recipient, question: 'Review this?', reasoning: REASON, wake: 'action' }, sender);
  await call('attach', { transport: 'raw' }, recipient);
  await call('hook', { format: 'json' }, recipient);
  await call('attach', claude, recipient);
  const blocked = await call('completion-check', event, recipient);
  assert.equal(blocked.decision, 'block');
  conforms('completion-check', blocked);
});

test('host-hook results satisfy their schema for every hook vendor', t => {
  for (const vendor of ['cursor', 'grok', 'claude', 'codex']) {
    const f = fixture(t, { git: true });
    const flags = ['--workspace', f.workspace, '--database', f.file];
    const names = vendor === 'cursor' ? ['sessionStart', 'beforeSubmitPrompt', 'postToolUse'] : vendor === 'grok' ? ['SessionStart', 'UserPromptSubmit', 'PostToolUse'] : ['SessionStart', 'UserPromptSubmit', 'PostToolUse'];
    const event = (name: string) => vendor === 'cursor'
      ? { hook_event_name: name, conversation_id: 'results-fixture', workspace_roots: [f.workspace] }
      : vendor === 'grok'
        ? { hook_event_name: name, hookEventName: name, sessionId: 'results-fixture', workspaceRoot: f.workspace }
        : { hook_event_name: name, session_id: 'results-fixture', cwd: f.workspace };
    const hook = (name: string): unknown => {
      const value: unknown = JSON.parse(execFileSync(binary, ['host-hook', '--vendor', vendor, ...flags], { input: JSON.stringify(event(name)), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
      conforms('host-hook', value);
      return value;
    };
    hook(names[0]!);
    hook(names[1]!);
    const db = new DatabaseSync(f.file);
    try {
      const host = (db.prepare('SELECT id FROM sessions WHERE vendor=?').get(vendor) as { id: string }).id;
      const sender = execFileSync(binary, ['join', JSON.stringify({ name: 'sender', vendor: 'raw' }), ...flags], { encoding: 'utf8' });
      const from = (JSON.parse(sender) as { id: string }).id;
      execFileSync(binary, ['send_message', JSON.stringify({ to: host, body: 'context for the host', reasoning: REASON }), ...flags, '--session', from], { encoding: 'utf8' });
    } finally {
      db.close();
    }
    const delivered = hook(names[2]!);
    assert.notDeepEqual(delivered, {}, `${vendor} delivers its context`);
    hook('SessionEnd');
  }
});

test('mcp frames satisfy their schema', t => {
  const f = fixture(t, { git: true });
  const frames = [
    { id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'results', version: '1' } } },
    { method: 'notifications/initialized' },
    { id: 2, method: 'tools/list' },
    { id: 3, method: 'tools/call', params: { name: 'peers', arguments: {} } },
    { id: 4, method: 'tools/call', params: { name: 'nope', arguments: {} } },
    { id: 5, method: 'unknown/method' },
  ].map(frame => JSON.stringify({ jsonrpc: '2.0', ...frame })).join('\n') + '\n';
  const out = execFileSync(binary, ['mcp', '--managed', '--name', 'results-mcp', '--vendor', 'generic', '--workspace', f.workspace, '--database', f.file], { input: frames, encoding: 'utf8', timeout: 15000, stdio: ['pipe', 'pipe', 'pipe'] });
  const printed = lines(out);
  assert.equal(printed.length, 5);
  for (const frame of printed) conforms('mcp', frame);
});

test('view and listen print values that satisfy their schemas', async t => {
  const f = fixture(t, { git: true });
  const call = cliCall(f.workspace, f.file);
  const bob = (await call('join', { name: 'bob', vendor: 'codex' })).id as string;
  await call('attach', { transport: 'raw' }, bob);
  const view = streaming(t, ['view', '{"open":false}', '--workspace', f.workspace, '--database', f.file]);
  await waitFor(() => view.output().length > 0, { message: view.stderr() || 'view did not print its address' });
  conforms('view', view.output()[0]);
  await view.stop();
  const listen = streaming(t, ['listen', '--workspace', f.workspace, '--database', f.file, '--session', bob, '--duration-ms', '600']);
  assert.equal(await listen.closed, 0, listen.stderr());
  const printed = listen.output();
  assert.ok(printed.length >= 1);
  for (const line of printed) conforms('listen', line);
});

test('run events satisfy their schema', { skip: process.platform === 'win32' }, async t => {
  const f = fixture(t, { git: true });
  const bin = join(f.workspace, 'bin'), script = join(f.workspace, 'codex-fixture.cjs'), log = join(f.workspace, 'requests.jsonl');
  mkdirSync(bin);
  writeFileSync(script, `const fs=require('node:fs');const rl=require('node:readline').createInterface({input:process.stdin});
const send=x=>console.log(JSON.stringify(x));rl.on('line',line=>{const r=JSON.parse(line);fs.appendFileSync(${JSON.stringify(log)},line+'\\n');if(r.id===undefined)return;
let result={};if(r.method==='config/read')result={config:{}};if(r.method==='skills/list')result={data:[]};if(r.method==='thread/start')result={thread:{id:'results-thread'}};
if(r.method==='turn/start')result={turn:{id:'results-turn',status:'inProgress'}};send({id:r.id,result});});`);
  const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
  writeFileSync(join(bin, 'codex'), `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`, { mode: 0o755 });
  const worker = streaming(t, ['run', '--vendor', 'codex', '--model', 'test', '--prompt', 'Remain active', '--duration-ms', '15000', '--workspace', f.workspace, '--database', f.file], { ...process.env, PATH: bin });
  const ready = await waitFor(() => worker.output().find((row): row is { session: string } => typeof row === 'object' && row !== null && 'type' in row && row.type === 'ready'), { timeout: 20000, message: worker.stderr() || 'worker did not become ready' });
  const sender = (await f.call('', 'join', { name: 'sender', vendor: 'generic' })).id as string;
  assert.ok(ready, 'the worker reported ready');
  await f.call(sender, 'send_message', { to: ready.session, body: 'during the turn', reasoning: REASON });
  await waitFor(() => worker.output().some(row => typeof row === 'object' && row !== null && 'type' in row && row.type === 'delivery'), { timeout: 15000, message: 'worker did not report a delivery' });
  await worker.stop();
  const events = worker.output();
  assert.ok(existsSync(log));
  for (const event of events) conforms('run', event);
});

test('every command was checked against its result schema', () => {
  assert.deepEqual(commands.filter(name => !covered.has(name)), []);
});
