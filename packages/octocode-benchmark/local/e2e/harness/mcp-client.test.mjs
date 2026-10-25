import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { EventEmitter } from 'node:events';
import { syncBuiltinESMExports } from 'node:module';
import test from 'node:test';
import { spawnCli, startServer } from './mcp-client.mjs';

function childFixture(t) {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  const child = new EventEmitter();
  child.pid = 123456;
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.stdin = new EventEmitter();
  child.stdin.write = t.mock.fn();
  child.kill = t.mock.fn();
  t.mock.method(childProcess, 'spawn', () => child);
  const spawnSync = t.mock.method(childProcess, 'spawnSync', () => ({ status: 0 }));
  const kill = t.mock.method(process, 'kill', () => true);
  const exitCode = process.exitCode;
  const cleanup = [];
  syncBuiltinESMExports();
  t.after(() => {
    for (const close of cleanup) close();
    child.emit('exit', 0, null);
    process.exitCode = exitCode;
    t.mock.restoreAll();
    syncBuiltinESMExports();
  });
  return { child, spawnSync, kill, cleanup };
}

test('CLI deadline terminates its owned process tree only when virtual time expires', async t => {
  const f = childFixture(t);
  const result = spawnCli(['probe'], { timeoutMs: 25_000 });
  f.child.stdout.emit('data', 'partial output');
  t.mock.timers.tick(24_999);
  assert.equal(f.kill.mock.callCount(), 0);
  assert.equal(f.spawnSync.mock.callCount(), 0);
  t.mock.timers.tick(1);
  if (process.platform === 'win32') {
    assert.deepEqual(f.spawnSync.mock.calls[0].arguments[1], ['/PID', '123456', '/T', '/F']);
  } else {
    assert.deepEqual(f.kill.mock.calls[0].arguments, [-123456, 'SIGKILL']);
  }
  f.child.emit('close', 137);
  assert.equal((await result).stdout, 'partial output');
});

test('CLI completion preserves output/status and clears its virtual deadline', async t => {
  const f = childFixture(t);
  const result = spawnCli(['probe'], { timeoutMs: 25_000 });
  f.child.stdout.emit('data', 'stdout');
  f.child.stderr.emit('data', 'stderr');
  f.child.emit('close', 7);
  const output = await result;
  assert.equal(output.status, 7);
  assert.equal(output.stdout, 'stdout');
  assert.equal(output.stderr, 'stderr');
  t.mock.timers.tick(100_000);
  assert.equal(f.kill.mock.callCount(), 0);
  assert.equal(f.spawnSync.mock.callCount(), 0);
});

test('CLI spawn failure clears its virtual deadline', async t => {
  const f = childFixture(t);
  const result = spawnCli(['probe'], { timeoutMs: 25_000 });
  const rejected = assert.rejects(result, /spawn failed/);
  f.child.emit('error', new Error('spawn failed'));
  await rejected;
  t.mock.timers.tick(100_000);
  assert.equal(f.kill.mock.callCount(), 0);
});

async function serverFixture(t) {
  const f = childFixture(t);
  const frames = [];
  f.child.stdin.write = frame => {
    const message = JSON.parse(frame);
    frames.push(message);
    if (message.method === 'initialize' || message.method === 'tools/list') {
      const result = message.method === 'initialize' ? {} : { tools: [] };
      queueMicrotask(() => f.child.stdout.emit('data', JSON.stringify({ jsonrpc: '2.0', id: message.id, result }) + '\n'));
    }
  };
  const client = await startServer({ timeoutMs: 25_000 });
  f.cleanup.push(() => client.close());
  return { ...f, client, frames };
}

test('MCP timeout cancels outstanding work and prevents overlapping follow-up requests', async t => {
  const { client, frames, kill } = await serverFixture(t);
  const result = client.raw('lspSearch', { queries: [] });
  const request = frames.at(-1);
  const sibling = client.rpc('tools/call', { name: 'fixture', arguments: {} }, 60_000);
  const rejected = assert.rejects(sibling, /timeout tools\/call after 25000ms/);
  t.mock.timers.tick(24_999);
  assert.equal(kill.mock.callCount(), 0);
  t.mock.timers.tick(1);
  assert.equal((await result).isError, true);
  await rejected;
  assert.deepEqual(frames.at(-1), { jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: request.id, reason: 'test request deadline exceeded' } });
  const sent = frames.length;
  assert.equal((await client.raw('lspSearch', { queries: [] })).isError, true);
  assert.equal(frames.length, sent, 'a timed-out session cannot accept another independent measurement');
  assert.deepEqual(kill.mock.calls.map(call => call.arguments), [[-123456, 'SIGTERM']]);
  t.mock.timers.tick(499);
  assert.equal(kill.mock.callCount(), 1);
  t.mock.timers.tick(1);
  assert.deepEqual(kill.mock.calls[1].arguments, [-123456, 'SIGKILL']);
  t.mock.timers.tick(60_000);
  assert.equal(kill.mock.callCount(), 2, 'retiring the session clears all pending deadlines');
});

test('MCP success clears its deadline and preserves the reusable session', async t => {
  const { client, frames, kill, child } = await serverFixture(t);
  for (let i = 0; i < 2; i++) {
    const response = client.rpc('tools/call', { name: 'fixture', arguments: {} });
    const request = frames.at(-1);
    child.stdout.emit('data', JSON.stringify({ jsonrpc: '2.0', id: request.id, result: { value: i } }) + '\n');
    assert.deepEqual((await response).result, { value: i });
    t.mock.timers.tick(25_000);
  }
  assert.equal(kill.mock.callCount(), 0);
  assert.equal(frames.some(frame => frame.method === 'notifications/cancelled'), false);
});

test('MCP initialization timeout retires the session without an invalid cancellation notification', async t => {
  const { child, kill } = childFixture(t);
  const frames = [];
  child.stdin.write = frame => frames.push(JSON.parse(frame));
  const response = startServer({ timeoutMs: 25_000 });
  const rejected = assert.rejects(response, /timeout initialize after 25000ms/);
  t.mock.timers.tick(25_000);
  await rejected;
  assert.deepEqual(frames.map(frame => frame.method), ['initialize']);
  assert.ok(kill.mock.callCount() > 0 || child.kill.mock.callCount() > 0);
});
