/**
 * Tests for the programmatic worker seams in agent-tools.ts:
 * - registerWorkerLedgerListener: subscribe/unsubscribe, event delivery, throwing-listener isolation.
 * - steerWorkerById / killWorkerById / getWorkerTranscript: reuse of the AgentMessage
 *   and /octocode-agents code paths, unknown-id handling.
 */
import assert from 'node:assert/strict';
import { test, beforeEach, afterEach } from 'vitest';
import {
  spawnRpcAgent,
  setAgentProcessFactoryForTests,
  isSubagentProcess,
  registerWorkerLedgerListener,
  steerWorkerById,
  killWorkerById,
  getWorkerTranscript,
  formatAgentLedgerDetails,
  pruneDroppableAgentsForSession,
  listWorkerLedgerEntries,
} from '../src/tools/agent-tools.js';
import type { WorkerLedgerEntry, WorkerLedgerEventType } from '../src/types.js';
import { emitAgentEnd, makeMockAgentProcess } from './helpers/mock-process.js';

// ─── Setup / teardown ─────────────────────────────────────────────────────────

beforeEach(() => {
  // Each test starts with a fresh empty agent registry
  setAgentProcessFactoryForTests(null);
});

afterEach(() => {
  setAgentProcessFactoryForTests(null);
});

// ─── pruneDroppableAgentsForSession ───────────────────────────────────────────

test('session prune drops killed workers but keeps live ones', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'to be killed', resourceMode: 'lean' });
  killWorkerById(record.id);
  // Killed → droppable; prune must remove it from the ledger.
  const removed = pruneDroppableAgentsForSession();
  assert.ok(removed >= 1, 'killed worker should be pruned');
  assert.equal(
    listWorkerLedgerEntries().some((e) => e.agentId === record.id),
    false,
    'pruned worker must not remain in the ledger',
  );
});

// ─── registerWorkerLedgerListener ─────────────────────────────────────────────

test('ledger listener receives entries and event types for worker transitions', () => {
  if (isSubagentProcess()) return;

  const seen: Array<{ type: WorkerLedgerEventType; entry: WorkerLedgerEntry }> = [];
  const unsubscribe = registerWorkerLedgerListener((entry, type) => {
    seen.push({ type, entry });
  });
  try {
    const mock = makeMockAgentProcess();
    setAgentProcessFactoryForTests(() => mock as never);
    const record = spawnRpcAgent({ task: 'listen to me', resourceMode: 'lean' });

    const types = seen.map((s) => s.type);
    assert.ok(types.includes('spawned'), `expected a 'spawned' event; got ${types.join(',')}`);
    assert.ok(types.includes('message'), `expected a 'message' event for the initial prompt; got ${types.join(',')}`);
    const initialMessage = seen.find((s) => s.type === 'message')?.entry.recentEvents.at(-1)?.message;
    assert.equal(initialMessage, 'initial prompt sent');
    assert.ok(seen.every((s) => s.entry.agentId === record.id), 'every entry carries the worker agentId');
    assert.equal(seen[0]!.entry.name, record.name);
  } finally {
    unsubscribe();
  }
});

test('ledger listener sees normalized-status flips (handback) via pushLedgerEvent', () => {
  if (isSubagentProcess()) return;

  const seen: Array<{ type: WorkerLedgerEventType; normalizedStatus?: string }> = [];
  const unsubscribe = registerWorkerLedgerListener((entry, type) => {
    seen.push({ type, normalizedStatus: entry.normalizedStatus });
  });
  try {
    const mock = makeMockAgentProcess();
    setAgentProcessFactoryForTests(() => mock as never);
    spawnRpcAgent({ task: 'flip status', resourceMode: 'lean' });

    mock._emit('stdout:data', Buffer.from(`${JSON.stringify({
      type: 'message_end',
      message: { role: 'assistant', content: [{ type: 'text', text: '[DONE] finished the thing' }] },
    })}\n`));

    const handback = seen.find((s) => s.type === 'handback');
    assert.ok(handback, 'normalized-status flip must emit a handback ledger event to listeners');
  } finally {
    unsubscribe();
  }
});

test('unsubscribed ledger listener stops receiving events', () => {
  if (isSubagentProcess()) return;

  let calls = 0;
  const unsubscribe = registerWorkerLedgerListener(() => {
    calls += 1;
  });
  unsubscribe();

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'nobody listening', resourceMode: 'lean' });

  assert.equal(calls, 0, 'unsubscribed listener must not be invoked');
});

test('a throwing ledger listener never breaks pushLedgerEvent or other listeners', () => {
  if (isSubagentProcess()) return;

  const seen: WorkerLedgerEventType[] = [];
  const unsubThrowing = registerWorkerLedgerListener(() => {
    throw new Error('listener boom');
  });
  const unsubGood = registerWorkerLedgerListener((_entry, type) => {
    seen.push(type);
  });
  try {
    const mock = makeMockAgentProcess();
    setAgentProcessFactoryForTests(() => mock as never);
    const record = spawnRpcAgent({ task: 'resilient ledger', resourceMode: 'lean' });

    // The ledger itself must still record events despite the throwing listener…
    assert.ok(record.ledgerEvents.length > 0, 'ledger events recorded despite throwing listener');
    // …and well-behaved listeners must still be invoked.
    assert.ok(seen.includes('spawned'), 'other listeners still receive events');
  } finally {
    unsubThrowing();
    unsubGood();
  }
});

// ─── Worker ledger ──────────────────────────────────────────────────────────

test('formatAgentLedgerDetails shows a branded running row while a worker is active', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'animate me', name: 'spark', resourceMode: 'lean' });

  const joined = formatAgentLedgerDetails();
  assert.match(joined, /^[✦✧✶✺✹✷]/m, 'running workers use the branded sparkle spinner');
  assert.match(joined, /spark/);
  assert.match(joined, /· running/);
});

// ─── steerWorkerById ──────────────────────────────────────────────────────────

test('steerWorkerById sends a steer RPC to a running worker', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'busy work', resourceMode: 'lean' });
  assert.equal(record.status, 'running');

  assert.equal(steerWorkerById(record.id, 'change course'), true);
  const steer = mock.writes.find((w) => w['type'] === 'steer');
  assert.ok(steer, 'a steer RPC must be written to worker stdin');
  assert.equal(steer!['message'], 'change course');
  assert.equal(record.ledgerEvents.at(-1)?.message, 'steer sent: change course');
  const outbound = listWorkerLedgerEntries().find((entry) => entry.agentId === record.id)?.lastMessage;
  assert.equal(outbound?.direction, 'to-agent');
  assert.equal(outbound?.action, 'steer');
  assert.equal(outbound?.preview, 'change course');
});

test('worker assistant output records an inbound reply for footer visibility', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'report back', resourceMode: 'lean' });
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({
    type: 'message_end',
    message: { role: 'assistant', content: [{ type: 'text', text: '[DONE] review complete' }] },
  })}\n`));

  const inbound = listWorkerLedgerEntries().find((entry) => entry.agentId === record.id)?.lastMessage;
  assert.equal(inbound?.direction, 'from-agent');
  assert.equal(inbound?.action, 'reply');
  assert.equal(inbound?.preview, '[DONE] review complete');
});

test('steerWorkerById queues via follow_up when the worker is idle', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'quick task', resourceMode: 'lean' });
  emitAgentEnd(mock);
  assert.equal(record.status, 'idle');

  assert.equal(steerWorkerById(record.id.slice(0, 8), 'next task please'), true);
  const followUp = mock.writes.find((w) => w['type'] === 'follow_up');
  assert.ok(followUp, 'idle worker must receive follow_up, not steer');
  assert.equal(followUp!['message'], 'next task please');
  assert.equal(mock.writes.find((w) => w['type'] === 'steer'), undefined);
  assert.equal(record.status, 'idle', 'queued follow_up does not fake a running turn before agent_start');
  assert.equal(record.pendingMessages, 1, 'queued follow_up is tracked until agent_start');
  assert.equal(record.ledgerEvents.at(-1)?.message, 'follow-up queued: next task please');
});

test('steerWorkerById returns false for unknown ids and empty messages', () => {
  if (isSubagentProcess()) return;

  assert.equal(steerWorkerById('no-such-agent', 'hello'), false);

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'still here', resourceMode: 'lean' });
  assert.equal(steerWorkerById(record.id, '   '), false, 'blank message is rejected');
});

test('steerWorkerById returns false when the worker process is dead', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'dies early', resourceMode: 'lean' });
  mock.exitCode = 0;
  mock._emit('close', 0, null);

  assert.equal(steerWorkerById(record.id, 'anyone home?'), false);
});

// ─── killWorkerById ───────────────────────────────────────────────────────────

test('killWorkerById kills a live worker by prefix and returns true', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'doomed worker', resourceMode: 'lean' });

  assert.equal(killWorkerById(record.id.slice(0, 8)), true);
  assert.equal(record.status, 'killed');
  assert.ok(record.ledgerEvents.some((e) => e.type === 'killed'));
});

test('killWorkerById returns false for unknown ids', () => {
  if (isSubagentProcess()) return;
  assert.equal(killWorkerById('does-not-exist'), false);
});

// ─── getWorkerTranscript ──────────────────────────────────────────────────────

test('getWorkerTranscript renders the single-agent status view', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'talkative', name: 'transcripty', resourceMode: 'lean' });
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({
    type: 'message_end',
    message: { role: 'assistant', content: [{ type: 'text', text: '[STATUS] deep in thought' }] },
  })}\n`));

  const transcript = getWorkerTranscript(record.id)!;
  assert.match(transcript, /transcripty/);
  assert.match(transcript, /agentId: /);
  assert.match(transcript, /deep in thought/);
});

test('getWorkerTranscript caps to the last maxLines lines', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'chatty', resourceMode: 'lean' });
  const output = Array.from({ length: 20 }, (_v, i) => `line ${i + 1}`).join('\n');
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({
    type: 'message_end',
    message: { role: 'assistant', content: [{ type: 'text', text: output }] },
  })}\n`));

  const full = getWorkerTranscript(record.id)!;
  const capped = getWorkerTranscript(record.id, { maxLines: 5 })!;
  assert.ok(full.split('\n').length > 5);
  assert.equal(capped.split('\n').length, 5);
  assert.match(capped, /line 20$/, 'keeps the freshest (last) lines');
});

test('getWorkerTranscript returns undefined for unknown ids', () => {
  if (isSubagentProcess()) return;
  assert.equal(getWorkerTranscript('missing-id'), undefined);
});
