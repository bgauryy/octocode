// SIGINT/SIGTERM end a one-shot command with 130/143; a loop (listen, mcp, view, run, dispatch) stops itself instead.
// Readiness is observed, never slept for: a command is past the signal guard once it printed its first output, or reported the guard
// through a preload ("inbox wait" is read-only and prints nothing until it ends).
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import type { ChildProcessWithoutNullStreams } from 'node:child_process';
import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import type { Socket } from 'node:net';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { binary, jsonCall, posix, spawn, tempWorkspace, waitFor } from './helpers.ts';

const runtime = await import('../dist/runtime.js');

// A preload that reports the moment the process installs its SIGINT handler, for commands with no other observable (the read-only "inbox wait").
const GUARD_PROBE = 'data:text/javascript,' + encodeURIComponent("const on = process.on.bind(process); process.on = (event, listener) => { const result = on(event, listener); if (event === 'SIGINT') process.stderr.write('signal-guard-installed\\n'); return result; };");

/** A started command: its captured output, when it closed and when it is past the signal guard. */
interface Started {
  child: ChildProcessWithoutNullStreams;
  stdout: string;
  stderr: string;
  closed: Promise<{ code: number | null; signal: NodeJS.Signals | null }>;
  ready: Promise<unknown>;
}
/** The observable that proves a command is past the signal guard. */
type Ready = 'stdout' | 'guard' | ((started: Started) => boolean);

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-signals-', { real: true });
  const home = join(workspace, 'home');
  const database = join(workspace, 'signals.sqlite');
  const env = { ...process.env, OCTOCODE_HOME: home };
  const raw = jsonCall(binary, workspace, database, { stdio: 'pipe', env });
  const call = (command: string, input?: unknown, session?: string): JsonRecord => raw(command, input, session);
  const a = call('join', { name: 'a', vendor: 'generic' }).id, b = call('join', { name: 'b', vendor: 'generic' }).id;
  /** Starts a command; `ready` is the observable that proves the process is past the signal guard. */
  const start = (args: string[], { ready = 'stdout', input = null, extraEnv = {} }: { ready?: Ready; input?: string | null; extraEnv?: NodeJS.ProcessEnv } = {}): Started => {
    const child = spawn(process.execPath, [...(ready === 'guard' ? ['--import', GUARD_PROBE] : []), binary, ...args, '--workspace', workspace, '--database', database], { env: { ...env, ...extraEnv }, stdio: ['pipe', 'pipe', 'pipe'] });
    t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); });
    const closed = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
    const result: Started = { child, stdout: '', stderr: '', closed, ready: Promise.resolve() };
    child.stdout.on('data', chunk => { result.stdout += chunk; });
    child.stderr.on('data', chunk => { result.stderr += chunk; });
    result.ready = waitFor(() => (ready === 'stdout' ? result.stdout.includes('\n') : ready === 'guard' ? result.stderr.includes('signal-guard-installed') : ready(result)), { timeout: 20000, message: args[0] + ' did not become ready' });
    if (input !== null) child.stdin.write(input);
    return result;
  };
  return { workspace, database, call, a, b, start };
}

const interrupt = async (process_: Started, signal: NodeJS.Signals) => {
  await process_.ready;
  process_.child.kill(signal);
  return process_.closed;
};

const interruptions: [NodeJS.Signals, number][] = [['SIGINT', 130], ['SIGTERM', 143]];
for (const [signal, code] of interruptions) {
  test(`${signal} ends the one-shot "inbox wait" with ${code}`, { skip: !posix }, async t => {
    const f = fixture(t);
    const command = f.start(['inbox', 'wait', '{"timeoutMs":60000}', '--session', f.a], { ready: 'guard' });
    assert.deepEqual(await interrupt(command, signal), { code, signal: null }, command.stderr);
  });

  test(`${signal} stops "listen" cleanly with 0`, { skip: !posix }, async t => {
    const f = fixture(t);
    f.call('attach', { transport: 'raw' }, f.a);
    const command = f.start(['listen', '--session', f.a]);
    assert.deepEqual(await interrupt(command, signal), { code: 0, signal: null }, command.stderr);
  });

  test(`${signal} stops the MCP server cleanly with 0`, { skip: !posix }, async t => {
    const f = fixture(t);
    const command = f.start(['mcp', '--session', f.a], { input: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }) + '\n' });
    assert.deepEqual(await interrupt(command, signal), { code: 0, signal: null }, command.stderr);
  });

  test(`${signal} stops "view" cleanly with 0`, { skip: !posix }, async t => {
    const f = fixture(t);
    const command = f.start(['view', '{"open":false}']);
    assert.deepEqual(await interrupt(command, signal), { code: 0, signal: null }, command.stderr);
  });

  test(`${signal} stops "run" cleanly with 0`, { skip: !posix }, async t => {
    const f = fixture(t);
    const bin = join(f.workspace, 'bin');
    mkdirSync(bin);
    writeFileSync(join(bin, 'claude'), '#!/bin/sh\nwhile true; do sleep 1; done\n');
    chmodSync(join(bin, 'claude'), 0o755);
    const command = f.start(['run', '--vendor', 'claude', '--model', 'm', '--prompt', 'wait for work'], { extraEnv: { PATH: bin + ':' + process.env.PATH } });
    assert.deepEqual(await interrupt(command, signal), { code: 0, signal: null }, command.stderr);
  });

  test(`${signal} interrupts "dispatch" with a delivery in flight: it stops, reports the interruption and exits 1`, { skip: !posix }, async t => {
    const f = fixture(t);
    const vendorSession = randomUUID();
    const endpoint = join(f.workspace, 'leader.sock');
    const frame = (value: JsonRecord) => { const body = Buffer.from(JSON.stringify(value)), size = Buffer.alloc(4); size.writeUInt32BE(body.length); return Buffer.concat([size, body]); };
    let prompted = false;
    const sockets = new Set<Socket>();
    // A Grok leader that accepts the prompt and never finishes the turn, so the delivery stays in flight.
    const server = createServer(socket => {
      sockets.add(socket);
      socket.on('error', () => {});
      let bytes = Buffer.alloc(0);
      socket.on('data', chunk => {
        bytes = Buffer.concat([bytes, chunk]);
        while (bytes.length >= 4 && bytes.length >= 4 + bytes.readUInt32BE(0)) {
          const size = bytes.readUInt32BE(0), message = JSON.parse(bytes.subarray(4, size + 4).toString());
          bytes = bytes.subarray(size + 4);
          if (message.type === 'register') { socket.write(frame({ type: 'registered', ready: true, leader_protocol_version: 1 })); continue; }
          if (message.type !== 'acp') continue;
          const rpc = JSON.parse(message.payload);
          const respond = (result: JsonRecord) => socket.write(frame({ type: 'acp', payload: JSON.stringify({ jsonrpc: '2.0', id: rpc.id, result }) }));
          if (rpc.method === 'initialize') respond({ protocolVersion: 1, agentCapabilities: { sessionCapabilities: { resume: {} } } });
          else if (rpc.method === '_x.ai/session/info') respond({ result: { sessionId: vendorSession, cwd: f.workspace } });
          else if (rpc.method === 'session/prompt') prompted = true;
        }
      });
    });
    await new Promise<void>(resolve => server.listen(endpoint, resolve));
    t.after(() => { for (const socket of sockets) socket.destroy(); server.close(); });
    const grok = f.call('join', { name: 'grok', vendor: 'grok' }).id;
    f.call('attach', { transport: 'grok', endpoint, vendorSession }, grok);
    f.call('send_message', { to: 'grok', body: 'Question requiring one answer', wake: 'action', reasoning: 'Keep one delivery in flight' }, f.a);
    const command = f.start(['dispatch', '{}', '--session', grok], { ready: () => prompted });
    const ended = await interrupt(command, signal);
    assert.deepEqual(ended, { code: 1, signal: null }, command.stderr);
    assert.match(command.stderr, /Dispatch interrupted; the recipient may still finish/);
  });
}

test('the signal guard defers to a loop only while the loop runs', () => {
  const { loopControl } = runtime;
  assert.equal(loopControl.loopActive(), false);
  const outer = loopControl.stopEvent(), inner = loopControl.stopEvent();
  assert.equal(loopControl.loopActive(), true);
  outer.dispose();
  assert.equal(loopControl.loopActive(), true, 'one loop still runs');
  inner.dispose();
  inner.dispose();
  assert.equal(loopControl.loopActive(), false, 'disposing twice is harmless and the last disposal re-arms the guard');
  assert.equal(process.listenerCount('SIGINT'), 0);
  assert.equal(process.listenerCount('SIGTERM'), 0);
});
