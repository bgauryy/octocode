import assert from 'node:assert/strict';
import { chmodSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { delimiter, join } from 'node:path';
import test from 'node:test';
import { only, posix, tempWorkspace, waitFor } from './helpers.ts';
import { fixture, runtime } from './store-support.ts';
import type { LoopWorker } from '../src/runtime/vendor-workers.ts';
import type { JsonRecord } from './json-record.ts';

const { proxy, timing, wire } = runtime;

const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch { return false; } };

/** A worker with nothing to deliver whose wire answers every poll with `event`; the loop reads only these members. */
function fakeWorker(event: () => Promise<null>): LoopWorker {
  return {
    identity: 'id', session: 'vendor-session', busy: false, deliverWhileBusy: true, args: {}, name: 'fake', prompt: '', toolNames: [],
    emit() {}, stageActionOnly: () => null, heartbeat: async () => {},
    start: async () => {}, prime: async peers => peers, begin: async () => {}, onEvent: async () => {}, offer: async () => {},
    store: {
      workspace: '', coordinationScope: '', known: () => ({ id: 'id', workspace: '', name: 'fake', vendor: 'generic', expiresAt: 0, task: '', status: 'available' }), peerContext: () => '', recordUsage: () => ({ recorded: true }), finishDispatch() {},
      stage: () => Object.assign([], { reason: undefined }),
    },
    host: { pid: undefined, event, send: async () => {} },
  };
}

test('the worker loop stops when its parent changes and runs until the deadline otherwise', { skip: !posix, timeout: 30000 }, async () => {
  let polled = 0;
  const orphan = fakeWorker(async () => { polled += 1; return null; });
  // No deadline and no stop: only the parent check can end this loop, so returning at all proves it, and the test timeout bounds a hang.
  await new proxy.Loop(orphan, { aborted: false }, null, process.ppid + 100000).run();
  assert.equal(polled, 0, 're-parented before the first poll');

  const owned = fakeWorker(async () => { polled += 1; await new Promise(resolve => setTimeout(resolve, 20)); return null; });
  await new proxy.Loop(owned, { aborted: false }, timing.monotonic() + 300, process.ppid).run();
  assert.ok(polled >= 3, 'polled until the deadline: ' + polled);

  // The stop event fires from inside the third poll, so the loop must end right after it, whatever the machine's speed.
  const stop = { aborted: false };
  polled = 0;
  const stopping = fakeWorker(async () => { polled += 1; if (polled === 3) stop.aborted = true; return null; });
  await new proxy.Loop(stopping, stop, null, process.ppid).run();
  assert.equal(polled, 3, 'a stop event ends the loop');
});

test('a child that parent-dies is detected through process.ppid changing', { skip: !posix }, async t => {
  // A grandchild reports its own parent; killing the middle process re-parents it, which is what Loop.run polls for.
  const { spawn } = await import('node:child_process');
  const program = `const { spawn } = require('node:child_process');
    const child = spawn(process.execPath, ['-e', 'const first = process.ppid; setInterval(() => { if (process.ppid !== first) { process.stdout.write("reparented\\\\n"); process.exit(0); } }, 20); process.stdout.write("watching " + first + "\\\\n");'], { stdio: ['ignore', 'inherit', 'inherit'] });
    setInterval(() => undefined, 1000);`;
  const middle = spawn(process.execPath, ['-e', program], { stdio: ['ignore', 'pipe', 'inherit'] });
  t.after(() => middle.kill('SIGKILL'));
  let text = '';
  middle.stdout.on('data', chunk => (text += chunk));
  await waitFor(() => text.includes('watching'));
  middle.kill('SIGKILL');
  await waitFor(() => text.includes('reparented'));
});

test('closing a vendor kills its whole process group, escalating to SIGKILL after two seconds', { skip: !posix }, async t => {
  const directory = tempWorkspace(t, 'proxy-workers-', { real: true });
  const pids = join(directory, 'pids');
  // The vendor and its grandchild ignore SIGTERM, so only the escalation ends them.
  const script = join(directory, 'stubborn.sh');
  writeFileSync(script, `#!/bin/sh\ntrap '' TERM\nsleep 100 &\necho $! > '${pids}'\necho ready\nwait\nwhile true; do sleep 1; done\n`);
  chmodSync(script, 0o755);
  const host = new wire.Wire(script, [], directory, [], null);
  await waitFor(() => existsSync(pids) && readFileSync(pids, 'utf8').trim() !== '');
  const grandchild = Number(readFileSync(pids, 'utf8'));
  const child = host.pid;
  assert.ok(child !== undefined);
  assert.ok(alive(child) && alive(grandchild));
  const started = performance.now();
  await host.close();
  const elapsed = performance.now() - started;
  assert.ok(elapsed >= wire.TERMINATE_GRACE_MS * 0.9, 'waited for SIGTERM to be honoured before SIGKILL: ' + elapsed);
  assert.ok(elapsed < wire.TERMINATE_GRACE_MS * 4, 'killed promptly after the grace period: ' + elapsed);
  await waitFor(() => !alive(child) && !alive(grandchild));
});

test('a cooperative vendor ends on SIGTERM without waiting for the grace period', { skip: !posix }, async t => {
  const directory = tempWorkspace(t, 'proxy-workers-', { real: true });
  const host = new wire.Wire('sleep', ['100'], directory, [], null);
  await waitFor(() => host.pid !== undefined);
  const pid = host.pid;
  assert.ok(pid !== undefined);
  const started = performance.now();
  await host.close();
  assert.ok(performance.now() - started < wire.TERMINATE_GRACE_MS, 'SIGTERM sufficed: SIGKILL escalation was not waited for');
  assert.equal(alive(pid), false);
});

test('usage is persisted once per source event and traced tool calls are paired', async t => {
  const f = fixture(t);
  const id = await f.join('worker');
  const usage = { input_tokens: 10, output_tokens: 4, cache_read_input_tokens: 3 };
  proxy.persistUsage(f.store, id, 'claude', 'vs', { type: 'result', usage, uuid: 'u-1' });
  proxy.persistUsage(f.store, id, 'claude', 'vs', { type: 'result', usage, uuid: 'u-1' });
  proxy.persistUsage(f.store, id, 'claude', 'vs', { type: 'assistant', message: { usage } });
  proxy.persistUsage(f.store, id, 'codex', 'vs', { method: 'thread/tokenUsage/updated', params: { tokenUsage: { total: { inputTokens: 9, outputTokens: 1 }, last: { inputTokens: 5 } } } });
  proxy.persistUsage(f.store, id, 'pi', 'vs', { type: 'message_end', uuid: 'p-1', message: { role: 'assistant', usage: { input: 5, output: 2, cacheRead: 1, cacheWrite: 2 } } });
  const records = f.store.db.all("SELECT data FROM records WHERE type='usage' ORDER BY id").map(row => JSON.parse(String(row.data)));
  assert.equal(records.length, 3, 'a repeated source event is idempotent');
  assert.deepEqual(records[0], { key: records[0].key, scope: 'turn', inputTokens: 10, outputTokens: 4, cachedInputTokens: 3 });
  const { key: cumulativeKey, ...cumulative } = records[1];
  assert.equal(typeof cumulativeKey, 'string');
  assert.notEqual(cumulativeKey, records[0].key, 'each usage record has its own key');
  assert.deepEqual(cumulative, { scope: 'cumulative', inputTokens: 9, outputTokens: 1, contextTokens: 5 });
  assert.equal(records[2].contextTokens, 8);
  assert.equal(records[2].cacheWriteTokens, 2);

  const calls = new Map();
  const started = proxy.traceTools('s1', { message: { content: [{ type: 'tool_use', id: 'c1', name: 'inbox', input: { a: 1 } }] } }, calls);
  assert.deepEqual(started.map(({ at, ...rest }) => (typeof at === 'number' ? rest : null)), [{ type: 'tool-call', callId: 'c1', tool: 'inbox', input: { a: 1 }, session: 's1' }]);
  const finished = proxy.traceTools('s1', { message: { content: [{ type: 'tool_result', tool_use_id: 'c1', content: 'ok', is_error: false }] } }, calls);
  assert.equal(finished[0].tool, 'inbox');
  assert.equal(calls.size, 0);
  const rpc = proxy.traceTools('s1', { method: 'item/started', params: { item: { type: 'mcpToolCall', id: 'r1', server: 'srv', tool: 'send', arguments: {} } } }, calls);
  assert.equal(rpc[0].server, 'srv');
  assert.deepEqual(proxy.traceTools('s1', { type: 'tool_execution_end', toolCallId: 'e1', toolName: 'x', result: 1, isError: false }, calls).map(record => record.type), ['tool-result']);
});

const FAKE_CODEX = `#!/usr/bin/env node
const fs = require('node:fs');
let buffer = '';
let turns = 0;
const send = value => process.stdout.write(JSON.stringify(value) + '\\n');
process.stdin.on('data', chunk => {
  buffer += chunk;
  for (let end = buffer.indexOf('\\n'); end >= 0; end = buffer.indexOf('\\n')) {
    const message = JSON.parse(buffer.slice(0, end));
    buffer = buffer.slice(end + 1);
    fs.appendFileSync(process.env.FAKE_CODEX_LOG, JSON.stringify(message) + '\\n');
    const reply = result => send({ id: message.id, result });
    if (message.method === 'initialize') reply({});
    else if (message.method === 'config/read') reply({ config: { plugins: { p: {} }, mcp_servers: { other: {} } } });
    else if (message.method === 'skills/list') reply({ data: [{ skills: [{ path: '/skill/one' }] }] });
    else if (message.method === 'thread/start') reply({ thread: { id: 'thr_fake' } });
    else if (message.method === 'thread/inject_items') reply({});
    else if (message.method === 'turn/start') {
      reply({ turn: { id: 't' + ++turns, status: 'inProgress' } });
      setTimeout(() => {
        send({ id: 'ask-1', method: 'item/commandExecution/requestApproval', params: {} });
        send({ method: 'item/completed', params: { item: { type: 'agentMessage', text: 'finished ' + turns } } });
        send({ method: 'turn/completed', params: { turn: { status: 'completed' } } });
      }, 60);
    }
  }
});
`;

test('a managed codex worker: handshake, first turn, mail delivered between turns, approvals refused, clean leave', { skip: !posix }, async t => {
  const f = fixture(t);
  const directory = tempWorkspace(t, 'proxy-workers-', { real: true });
  const log = join(directory, 'frames.log');
  const script = join(directory, 'codex');
  writeFileSync(script, FAKE_CODEX);
  chmodSync(script, 0o755);
  const saved = { PATH: process.env.PATH, FAKE_CODEX_LOG: process.env.FAKE_CODEX_LOG };
  process.env.PATH = directory + delimiter + process.env.PATH;
  process.env.FAKE_CODEX_LOG = log;
  t.after(() => { process.env.PATH = saved.PATH; if (saved.FAKE_CODEX_LOG === undefined) delete process.env.FAKE_CODEX_LOG; else process.env.FAKE_CODEX_LOG = saved.FAKE_CODEX_LOG; });
  const sender = await f.join('sender');
  const output: JsonRecord[] = [];
  const io = {
    output: (value: JsonRecord) => output.push(value),
    open: (path: string, workspace: string, options: { readOnly?: boolean; create?: boolean } = {}) => new runtime.store.Store(path, workspace, options.readOnly ?? false, options.create ?? false),
    launcher: { mcp: (args: readonly string[]) => ({ command: 'communication', args: [...args] }), binary: 'communication', piExtension: '' },
  };
  const running = proxy.run({ vendor: 'codex', model: 'gpt-test', prompt: 'Do the task\nwith details', durationMs: 4000, workspace: f.workspace, database: f.file, trace: true }, io);
  await waitFor(() => output.some(record => record.type === 'turn-completed')).catch(error => { throw new Error(error.message + ' ' + JSON.stringify(output) + ' ' + (existsSync(log) ? readFileSync(log, 'utf8') : 'nolog')); });
  const ready = output.find(record => record.type === 'ready');
  assert.ok(ready);
  assert.equal(ready.vendor, 'codex');
  assert.equal(ready.leases, 'managed');
  assert.ok(Number.isInteger(ready.pid));
  const identity = ready.session;
  const row = only(f.store.db.raw.prepare('SELECT vendor,vendorSession FROM sessions WHERE id=?').get(identity));
  assert.deepEqual({ ...row }, { vendor: 'codex', vendorSession: 'thr_fake' });
  assert.equal(only(f.store.db.raw.prepare('SELECT transport FROM attachments WHERE session=?').get(identity)).transport, 'raw');
  assert.equal(runtime.ownerLock.deliveryOwnerLive(f.file, identity), true, 'the worker owns delivery for its session');

  const sent = await f.call(sender, 'send_message', { to: identity, body: 'peer fact for the worker', wake: 'action' });
  await waitFor(() => output.some(record => record.type === 'delivery' && record.messages.includes(sent.id)));
  await waitFor(() => output.filter(record => record.type === 'turn-completed').length >= 2);
  await running;

  const frames = readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line));
  const methods = frames.map(frame => frame.method).filter(Boolean);
  assert.deepEqual(methods.slice(0, 7), ['initialize', 'initialized', 'config/read', 'skills/list', 'thread/start', 'thread/inject_items', 'turn/start']);
  const thread = frames.find(frame => frame.method === 'thread/start').params;
  assert.equal(thread.model, 'gpt-test');
  assert.equal(thread.approvalPolicy, 'never');
  assert.deepEqual(thread.config.skills.config, [{ path: '/skill/one', enabled: false }]);
  assert.deepEqual(thread.config.plugins, { p: { enabled: false } });
  assert.deepEqual(Object.keys(thread.config.mcp_servers), ['other', 'agents_communication']);
  assert.deepEqual(thread.config.mcp_servers.agents_communication.args.slice(0, 7), ['mcp', '--workspace', f.workspace, '--database', f.file, '--session', identity]);
  const first = frames.find(frame => frame.method === 'turn/start').params;
  assert.match(first.input[0].text, /User task:\nDo the task\nwith details/);
  assert.match(first.input[0].text, new RegExp(`"session":"${identity}"`));
  const second = frames.filter(frame => frame.method === 'turn/start')[1].params;
  assert.match(second.toolOutput.output, /peer fact for the worker/);
  const refusals = frames.filter(frame => frame.id === 'ask-1');
  assert.ok(refusals.length >= 1);
  assert.equal(refusals[0].error.code, -32601);
  assert.equal(only(f.store.db.raw.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id)).state, 'submitted');
  assert.ok(output.some(record => record.type === 'text' && /finished/.test(record.text)));
  assert.ok(output.some(record => record.type === 'protocol' && record.method === 'turn/completed'));
  assert.equal(runtime.ownerLock.deliveryOwnerLive(f.file, identity), false, 'released when the worker ends');
  assert.ok(Number(only(f.store.db.raw.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(identity)).expiresAt) <= Date.now(), 'the worker left');
  assert.equal(alive(ready.pid), false, 'the vendor process is gone');
});
