/**
 * Tests for the agent inbox: pure item builders, the 2-stage overlay flow with
 * injected fakes, the notification decision rules, and the registerAgentInbox
 * wiring (turn tracking + shutdown suppress race).
 */
import assert from 'node:assert/strict';
import { test, beforeEach } from 'vitest';
import {
  LONG_RUN_NOTIFY_MS,
  buildInboxActionItems,
  buildInboxItems,
  inboxDisplayState,
  inboxSummaryLine,
  registerAgentInbox,
  runAgentInboxOverlay,
  shouldNotifyWorkerEvent,
  type AgentInboxDeps,
} from '../src/tools/agent-inbox.js';
import {
  clearTitleFlashTimer,
  resumeDesktopNotificationsForTests,
} from '../src/tools/desktop-notify.js';
import type { PiContext, PiInstance, WorkerLedgerEntry, WorkerLedgerEventType } from '../src/types.js';

beforeEach(() => {
  resumeDesktopNotificationsForTests();
  clearTitleFlashTimer();
});

const NOW = Date.parse('2026-08-19T12:01:00.000Z');

function makeEntry(overrides: Partial<WorkerLedgerEntry> = {}): WorkerLedgerEntry {
  return {
    agentId: 'aaaabbbb-cccc-dddd-eeee-ffff00001111',
    name: 'atlas',
    status: 'running',
    startedAt: new Date(NOW - 60_000).toISOString(),
    updatedAt: new Date(NOW - 5_000).toISOString(),
    recentEvents: [],
    ...overrides,
  };
}

// ─── buildInboxItems / display state / summary ────────────────────────────────

test('buildInboxItems: running worker shows spinner glyph, id prefix, live age, and deltaSummary', () => {
  const entry = makeEntry({ deltaSummary: '[STATUS] scanning tests' });
  const [item] = buildInboxItems([entry], NOW);
  assert.equal(item!.value, entry.agentId);
  assert.ok(item!.label.startsWith('⟳ atlas (aaaabbbb)'), item!.label);
  assert.ok(item!.label.includes('· running ·'));
  assert.ok(item!.label.includes('1m'), 'age computed from startedAt against now');
  assert.equal(item!.description, '[STATUS] scanning tests');
});

test('buildInboxItems: done worker freezes age at updatedAt and shows the result summary', () => {
  const entry = makeEntry({
    status: 'exited',
    normalizedStatus: 'done',
    result: 'All 12 tests pass',
    startedAt: new Date(NOW - 120_000).toISOString(),
    updatedAt: new Date(NOW - 90_000).toISOString(),
  });
  const [item] = buildInboxItems([entry], NOW);
  assert.ok(item!.label.startsWith('✓ atlas'), item!.label);
  assert.ok(item!.label.includes('· done ·'));
  assert.ok(item!.label.includes('30s'), `age frozen at updatedAt-startedAt: ${item!.label}`);
  assert.equal(item!.description, 'All 12 tests pass');
});

test('buildInboxItems: failed / killed / idle glyphs and states', () => {
  const items = buildInboxItems([
    makeEntry({ agentId: 'f'.repeat(36), status: 'failed' }),
    makeEntry({ agentId: 'k'.repeat(36), status: 'killed' }),
    makeEntry({ agentId: 'i'.repeat(36), status: 'idle' }),
  ], NOW);
  assert.ok(items[0]!.label.startsWith('✗') && items[0]!.label.includes('· failed ·'));
  assert.ok(items[1]!.label.startsWith('⊘') && items[1]!.label.includes('· killed ·'));
  assert.ok(items[2]!.label.startsWith('◎') && items[2]!.label.includes('· idle ·'));
});

test('inboxDisplayState: normalizedStatus refines the raw process status', () => {
  assert.equal(inboxDisplayState({ status: 'idle', normalizedStatus: 'done' }), 'done');
  assert.equal(inboxDisplayState({ status: 'idle', normalizedStatus: 'blocked' }), 'blocked');
  assert.equal(inboxDisplayState({ status: 'idle', normalizedStatus: 'failed' }), 'failed');
  assert.equal(inboxDisplayState({ status: 'killed', normalizedStatus: 'done' }), 'killed');
  assert.equal(inboxDisplayState({ status: 'starting' }), 'starting');
  // Exited beats blocked: a dead process that last said [BLOCKED] cannot be
  // steered, so it must not present as an actionable blocked worker.
  assert.equal(inboxDisplayState({ status: 'exited', normalizedStatus: 'blocked' }), 'done');
});

test('inboxSummaryLine: flattens newlines, caps length, and falls back to the last ledger event', () => {
  const long = makeEntry({ status: 'exited', result: `line one\nline two ${'x'.repeat(200)}` });
  const summary = inboxSummaryLine(long);
  assert.ok(!summary.includes('\n'));
  assert.equal(summary.length, 90);
  assert.ok(summary.endsWith('…'));

  const fallback = makeEntry({
    status: 'exited',
    recentEvents: [
      { type: 'spawned', timestamp: NOW - 1000, message: 'spawned atlas' },
      { type: 'exit', timestamp: NOW, message: 'process closed with code 0' },
    ],
  });
  assert.equal(inboxSummaryLine(fallback), 'process closed with code 0');

  // deltaSummary only counts while the worker is live.
  const stale = makeEntry({ status: 'exited', deltaSummary: '[STATUS] old note', result: 'final answer' });
  assert.equal(inboxSummaryLine(stale), 'final answer');
});

test('buildInboxActionItems: live workers get steer/kill, terminal workers only view/dismiss', () => {
  const live = buildInboxActionItems({ status: 'running' }).map((i) => i.value);
  assert.deepEqual(live, ['view', 'steer', 'kill', 'dismiss']);
  const idle = buildInboxActionItems({ status: 'idle' }).map((i) => i.value);
  assert.deepEqual(idle, ['view', 'steer', 'kill', 'dismiss']);
  const done = buildInboxActionItems({ status: 'exited' }).map((i) => i.value);
  assert.deepEqual(done, ['view', 'dismiss']);
  const killed = buildInboxActionItems({ status: 'killed' }).map((i) => i.value);
  assert.deepEqual(killed, ['view', 'dismiss']);
});

// ─── runAgentInboxOverlay (2-stage flow with fakes) ───────────────────────────

interface FlowFakes {
  deps: AgentInboxDeps;
  notifications: Array<{ msg: string; level?: string }>;
  steered: Array<{ id: string; message: string }>;
  killed: string[];
  transcripts: Array<{ id: string; maxLines?: number }>;
  overlayCalls: Array<{ title: string; values: string[] }>;
}

function makeFlow(
  entries: WorkerLedgerEntry[],
  overlayResults: Array<string | null | undefined>,
  opts: { input?: string | undefined; noInputFn?: boolean; editor?: string } = {},
): FlowFakes {
  const notifications: FlowFakes['notifications'] = [];
  const steered: FlowFakes['steered'] = [];
  const killed: string[] = [];
  const transcripts: FlowFakes['transcripts'] = [];
  const overlayCalls: FlowFakes['overlayCalls'] = [];
  const ui: Record<string, unknown> = {
    editor: async (_t: string, _p?: string) => opts.editor,
  };
  if (!opts.noInputFn) ui['input'] = async (_t: string, _p?: string) => opts.input;
  const ctx = { hasUI: true, ui } as unknown as PiContext;
  const deps: AgentInboxDeps = {
    ctx,
    listEntries: () => entries,
    runOverlay: async (_ctx, o) => {
      overlayCalls.push({ title: o.title, values: o.items.map((i) => i.value) });
      return overlayResults.shift();
    },
    steer: (id, message) => { steered.push({ id, message }); return true; },
    kill: (id) => { killed.push(id); return true; },
    transcript: (id, o) => { transcripts.push({ id, maxLines: o?.maxLines }); return `TRANSCRIPT:${id.slice(0, 8)}`; },
    notify: (_ctx, msg, level) => { notifications.push({ msg, level }); },
    now: () => NOW,
  };
  return { deps, notifications, steered, killed, transcripts, overlayCalls };
}

test('overlay flow: empty inbox notifies and never opens an overlay', async () => {
  const flow = makeFlow([], []);
  await runAgentInboxOverlay(flow.deps);
  assert.equal(flow.overlayCalls.length, 0);
  assert.equal(flow.notifications.length, 1);
  assert.ok(flow.notifications[0]!.msg.includes('no spawned workers'));
});

test('overlay flow: steer path prompts for a message and sends it to the picked worker', async () => {
  const entry = makeEntry();
  const flow = makeFlow([entry], [entry.agentId, 'steer'], { input: 'focus on the failing test' });
  await runAgentInboxOverlay(flow.deps);
  assert.equal(flow.overlayCalls.length, 2);
  assert.deepEqual(flow.overlayCalls[1]!.values, ['view', 'steer', 'kill', 'dismiss']);
  assert.deepEqual(flow.steered, [{ id: entry.agentId, message: 'focus on the failing test' }]);
  assert.ok(flow.notifications.some((n) => n.msg.includes('Steer sent to atlas')));
});

test('overlay flow: steer falls back to the editor when ctx.ui.input is unavailable', async () => {
  const entry = makeEntry();
  const flow = makeFlow([entry], [entry.agentId, 'steer'], { noInputFn: true, editor: 'from editor' });
  await runAgentInboxOverlay(flow.deps);
  assert.deepEqual(flow.steered, [{ id: entry.agentId, message: 'from editor' }]);
});

test('overlay flow: cancelled/empty steer prompt sends nothing', async () => {
  const entry = makeEntry();
  const flow = makeFlow([entry], [entry.agentId, 'steer'], { input: '   ' });
  await runAgentInboxOverlay(flow.deps);
  assert.deepEqual(flow.steered, []);
  assert.ok(flow.notifications.some((n) => n.msg.includes('Steer cancelled')));
});

test('overlay flow: kill path calls kill with the picked worker id', async () => {
  const entry = makeEntry();
  const flow = makeFlow([entry], [entry.agentId, 'kill']);
  await runAgentInboxOverlay(flow.deps);
  assert.deepEqual(flow.killed, [entry.agentId]);
  assert.ok(flow.notifications.some((n) => n.level === 'warning' && n.msg.includes('Killed worker atlas')));
});

test('overlay flow: view path fetches a line-capped transcript and notifies it', async () => {
  const entry = makeEntry();
  const flow = makeFlow([entry], [entry.agentId, 'view']);
  await runAgentInboxOverlay(flow.deps);
  assert.equal(flow.transcripts.length, 1);
  assert.equal(flow.transcripts[0]!.id, entry.agentId);
  assert.ok((flow.transcripts[0]!.maxLines ?? 0) > 0);
  assert.ok(flow.notifications.some((n) => n.msg.startsWith('TRANSCRIPT:')));
});

test('overlay flow: cancel at stage 1 or dismiss at stage 2 does nothing', async () => {
  const entry = makeEntry();
  const cancelled = makeFlow([entry], [null]);
  await runAgentInboxOverlay(cancelled.deps);
  assert.equal(cancelled.overlayCalls.length, 1);
  assert.deepEqual(cancelled.notifications, []);

  const dismissed = makeFlow([entry], [entry.agentId, 'dismiss']);
  await runAgentInboxOverlay(dismissed.deps);
  assert.deepEqual(dismissed.steered, []);
  assert.deepEqual(dismissed.killed, []);
  assert.deepEqual(dismissed.notifications, []);
});

// ─── shouldNotifyWorkerEvent (pure decision) ──────────────────────────────────

const base = { turnActive: false, suppressed: false, alreadyNotified: false, now: NOW };

test('shouldNotifyWorkerEvent: exit while idle notifies; suppress wins over everything', () => {
  const entry = makeEntry({ status: 'exited' });
  assert.equal(shouldNotifyWorkerEvent(entry, 'exit', base), true);
  assert.equal(shouldNotifyWorkerEvent(entry, 'exit', { ...base, suppressed: true }), false);
  assert.equal(shouldNotifyWorkerEvent(entry, 'exit', { ...base, alreadyNotified: true }), false);
});

test('shouldNotifyWorkerEvent: killed events never notify', () => {
  const entry = makeEntry({ status: 'killed' });
  assert.equal(shouldNotifyWorkerEvent(entry, 'killed', base), false);
  assert.equal(shouldNotifyWorkerEvent(entry, 'killed', { ...base, turnActive: true }), false);
  // The process close handler emits a type:'exit' ledger event even for a
  // killed worker — the STATUS must suppress it, or a manual kill flashes a
  // misleading "worker finished" desktop notification.
  assert.equal(shouldNotifyWorkerEvent(entry, 'exit', base), false);
  assert.equal(shouldNotifyWorkerEvent(entry, 'error', base), false);
});

test('shouldNotifyWorkerEvent: error only counts once the worker is failed', () => {
  assert.equal(shouldNotifyWorkerEvent(makeEntry({ status: 'failed' }), 'error', base), true);
  assert.equal(shouldNotifyWorkerEvent(makeEntry({ status: 'running' }), 'error', base), false);
  assert.equal(shouldNotifyWorkerEvent(makeEntry({ status: 'idle' }), 'status', base), false);
  assert.equal(shouldNotifyWorkerEvent(makeEntry({ status: 'idle' }), 'handback', base), false);
});

test('shouldNotifyWorkerEvent: mid-turn completions only notify for long runs (> 30s)', () => {
  const shortRun = makeEntry({ status: 'exited', startedAt: new Date(NOW - 5_000).toISOString() });
  assert.equal(shouldNotifyWorkerEvent(shortRun, 'exit', { ...base, turnActive: true }), false);
  const longRun = makeEntry({ status: 'exited', startedAt: new Date(NOW - LONG_RUN_NOTIFY_MS - 1_000).toISOString() });
  assert.equal(shouldNotifyWorkerEvent(longRun, 'exit', { ...base, turnActive: true }), true);
});

// ─── registerAgentInbox (wiring + shutdown race) ──────────────────────────────

interface FakePi {
  pi: PiInstance;
  handlers: Record<string, (event: unknown, ctx?: PiContext) => Promise<unknown> | unknown>;
  commands: Record<string, { description: string; handler: (args: string, ctx: unknown) => Promise<void> }>;
}

function makeFakePi(): FakePi {
  const handlers: FakePi['handlers'] = {};
  const commands: FakePi['commands'] = {};
  const pi = {
    on(event: string, handler: (...args: unknown[]) => unknown) {
      handlers[event] = handler as FakePi['handlers'][string];
    },
    registerCommand(name: string, opts: FakePi['commands'][string]) {
      commands[name] = opts;
    },
  } as unknown as PiInstance;
  return { pi, handlers, commands };
}

interface Harness {
  fakePi: FakePi;
  emit: (entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void;
  oscMessages: string[];
  flashes: string[];
  notifications: Array<{ msg: string; level?: string }>;
  unsubCalls: number[];
  registration: ReturnType<typeof registerAgentInbox>;
}

function makeHarness(entries: WorkerLedgerEntry[] = []): Harness {
  const fakePi = makeFakePi();
  const oscMessages: string[] = [];
  const flashes: string[] = [];
  const notifications: Harness['notifications'] = [];
  const unsubCalls: number[] = [];
  let listener: ((entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void) | undefined;
  const registration = registerAgentInbox(
    fakePi.pi,
    (_ctx, msg, level) => { notifications.push({ msg, level }); },
    {
      registerListener: (cb) => {
        listener = cb;
        return () => { unsubCalls.push(1); };
      },
      listEntries: () => entries,
      emitOsc9: (m) => { oscMessages.push(m); },
      flashTitle: (_ctx, t) => { flashes.push(t); },
      notificationsEnabled: () => true,
      now: () => NOW,
    },
  );
  return {
    fakePi,
    emit: (entry, type) => listener?.(entry, type),
    oscMessages,
    flashes,
    notifications,
    unsubCalls,
    registration,
  };
}

test('registerAgentInbox: worker exit while idle fires OSC + title flash + one notify, deduped per worker', () => {
  const h = makeHarness();
  const entry = makeEntry({ status: 'exited', normalizedStatus: 'done', result: 'refactor complete' });
  h.emit(entry, 'exit');
  assert.equal(h.oscMessages.length, 1);
  assert.ok(h.oscMessages[0]!.includes('atlas') && h.oscMessages[0]!.includes('finished'));
  assert.equal(h.flashes.length, 1);
  assert.equal(h.notifications.length, 1);
  assert.ok(h.notifications[0]!.msg.includes('refactor complete'), 'one-liner includes the result summary');
  assert.equal(h.notifications[0]!.level, 'info');

  // A duplicate completion event for the same worker must not notify again.
  h.emit(entry, 'exit');
  assert.equal(h.notifications.length, 1);
  h.registration.shutdown();
});

test('registerAgentInbox: failed worker notifies at warning level', () => {
  const h = makeHarness();
  h.emit(makeEntry({ status: 'failed' }), 'error');
  assert.equal(h.notifications.length, 1);
  assert.equal(h.notifications[0]!.level, 'warning');
  assert.ok(h.oscMessages[0]!.includes('failed'));
  h.registration.shutdown();
});

test('registerAgentInbox: mid-turn short-run completions stay silent, long runs ping', async () => {
  const h = makeHarness();
  await h.fakePi.handlers['agent_start']?.({}, { hasUI: true } as unknown as PiContext);
  h.emit(makeEntry({ status: 'exited', startedAt: new Date(NOW - 3_000).toISOString() }), 'exit');
  assert.equal(h.notifications.length, 0, 'short run mid-turn is silent');

  h.emit(makeEntry({ agentId: 'l'.repeat(36), status: 'exited', startedAt: new Date(NOW - 60_000).toISOString() }), 'exit');
  assert.equal(h.notifications.length, 1, 'long run notifies even mid-turn');

  await h.fakePi.handlers['agent_end']?.({}, { hasUI: true } as unknown as PiContext);
  h.emit(makeEntry({ agentId: 's'.repeat(36), status: 'exited', startedAt: new Date(NOW - 3_000).toISOString() }), 'exit');
  assert.equal(h.notifications.length, 2, 'after agent_end the turn is no longer active');
  h.registration.shutdown();
});

test('registerAgentInbox shutdown race: post-suppress killed/exit ledger events are ignored entirely', () => {
  const h = makeHarness();
  h.registration.shutdown();
  assert.equal(h.unsubCalls.length, 1, 'ledger listener unsubscribed');

  // Even if an in-flight callback still holds the listener, suppress blocks it.
  h.emit(makeEntry({ status: 'killed' }), 'killed');
  h.emit(makeEntry({ status: 'exited' }), 'exit');
  h.emit(makeEntry({ status: 'failed' }), 'error');
  assert.deepEqual(h.oscMessages, []);
  assert.deepEqual(h.flashes, []);
  assert.deepEqual(h.notifications, []);

  // shutdown is idempotent.
  h.registration.shutdown();
  assert.equal(h.unsubCalls.length, 1);
});

test('registerAgentInbox resume: re-arms notifications and re-subscribes after shutdown (session_start counterpart)', () => {
  const h = makeHarness();
  h.registration.shutdown();
  assert.equal(h.unsubCalls.length, 1, 'shutdown detached the ledger listener');

  // A following session (/new, /resume, /fork) fires session_start → resume():
  // it must lift both suppress flags AND re-attach the ledger listener so a
  // worker completing in this new session notifies again. Without the fix the
  // once-per-process registration stayed muted+detached forever.
  h.registration.resume();
  h.emit(makeEntry({ agentId: 'a'.repeat(36), status: 'exited', normalizedStatus: 'done' }), 'exit');
  assert.equal(h.notifications.length, 1, 'worker completion notifies again after resume');
  assert.equal(h.oscMessages.length, 1, 'OSC re-armed');
  assert.equal(h.flashes.length, 1, 'title flash re-armed');

  // resume() is idempotent: a second call while already attached must not
  // re-subscribe (a duplicate listener would be torn down separately, so the
  // final shutdown would report more than one unsubscribe).
  h.registration.resume();
  h.emit(makeEntry({ agentId: 'b'.repeat(36), status: 'exited' }), 'exit');
  assert.equal(h.notifications.length, 2, 'exactly one notify per event — no duplicate listener');

  h.registration.shutdown();
  assert.equal(h.unsubCalls.length, 2, 'resume re-subscribed exactly one live listener');
});

test('registerAgentInbox leaves session_shutdown ownership to the extension lifecycle', () => {
  const h = makeHarness();
  assert.equal(h.fakePi.handlers['session_shutdown'], undefined);
  h.registration.shutdown();
  assert.equal(h.unsubCalls.length, 1);
  h.emit(makeEntry({ status: 'exited' }), 'exit');
  assert.deepEqual(h.notifications, []);
});
