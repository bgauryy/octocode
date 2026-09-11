import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
import { openAwarenessStore } from '@octocodeai/octocode-awareness/host';
import type { PiContext } from '../src/types.js';
import { openPersistentAwareness } from '../src/tools/storage-policy.js';
import {
  clearAwarenessCacheEntry,
  getCachedAwarenessStatus,
  refreshAwarenessPanel,
  resetAwarenessStatusStateForTests,
  resumeAwarenessPanel,
  setAwarenessMetricsRefreshForUi,
  setAwarenessStatusRunnerForTests,
  suppressAwarenessPanel,
} from '../src/tools/awareness-status.js';
import { registerAwarenessEventConsumer } from '../src/tools/awareness-event-consumer.js';

let root: string;
let priorHome: string | undefined;
let priorMode: string | undefined;

beforeEach(() => {
  root = mkdtempSync(path.join(tmpdir(), 'pi-awareness-policy-'));
  priorHome = process.env.OCTOCODE_HOME;
  priorMode = process.env.OCTOCODE_STORAGE_MODE;
  process.env.OCTOCODE_HOME = path.join(root, 'home');
  process.env.OCTOCODE_STORAGE_MODE = 'memory';
});

afterEach(() => {
  resetAwarenessStatusStateForTests();
  setAwarenessMetricsRefreshForUi(undefined);
  resumeAwarenessPanel();
  if (priorHome === undefined) delete process.env.OCTOCODE_HOME; else process.env.OCTOCODE_HOME = priorHome;
  if (priorMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE; else process.env.OCTOCODE_STORAGE_MODE = priorMode;
  rmSync(root, { recursive: true, force: true });
});

test('memory storage mode rejects durable Awareness opens', () => {
  assert.throws(
    () => openPersistentAwareness({ workspace: root }),
    /Persistent storage is disabled/,
  );
});

test('clearing a status entry invalidates an older async completion', async () => {
  let resolve!: (value: any) => void;
  setAwarenessStatusRunnerForTests(() => new Promise((done) => { resolve = done; }));
  let repaints = 0;
  setAwarenessMetricsRefreshForUi(() => { repaints += 1; });
  const ctx = { cwd: root, hasUI: true } as PiContext;
  refreshAwarenessPanel(ctx);
  assert.equal(repaints, 1);
  suppressAwarenessPanel();
  clearAwarenessCacheEntry(root);
  resumeAwarenessPanel();
  resolve({ activePlans: 1, readyTasks: 0, inProgressTasks: 0, verifyTasks: 0, lockCount: 0, workCount: 0, agentCount: 0, messageCount: 0, unreadInbox: 0 });
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(getCachedAwarenessStatus(root), null);
  assert.equal(repaints, 1);
});

test('event consumers bind delivery to the latest context and recipient identity', async () => {
  process.env.OCTOCODE_STORAGE_MODE = 'persistent';
  const workspace = path.join(root, 'workspace');
  mkdirSync(workspace);
  const dbPath = path.join(root, 'awareness.sqlite3');
  const sessionFile = path.join(root, 'session.jsonl');
  writeFileSync(sessionFile, '');
  let entries: any[] = [];
  const sent: any[] = [];
  const handlers = new Map<string, (...args: any[]) => Promise<void>>();
  let recipient = 'first';
  const pi = {
    on(name: string, handler: (...args: any[]) => Promise<void>) { handlers.set(name, handler); },
    sendMessage(message: any) { sent.push(message); entries.push({ type: 'custom_message', ...message }); },
  };
  registerAwarenessEventConsumer(pi as never, {
    openStore: () => openAwarenessStore({ workspace, dbPath }),
    resolveExpectedAgentId: () => recipient,
  });
  const context = (captured = entries) => ({
    cwd: workspace,
    sessionManager: {
      getSessionId: () => 'stable',
      getSessionFile: () => sessionFile,
      getEntries: () => captured,
    },
  }) as PiContext;
  const seed = (to: string, text: string) => {
    const store = openAwarenessStore({ workspace, dbPath });
    try { store.sendMessage({ fromAgentId: 'peer', toAgentId: to, topic: 'EVIDENCE', text }); } finally { store.close(); }
  };
  seed('first', 'before resume');
  await handlers.get('session_start')!({}, context());
  entries = [...entries];
  seed('first', 'after resume');
  await handlers.get('session_start')!({}, context());
  recipient = 'second';
  seed('second', 'new recipient');
  await handlers.get('agent_end')!({}, context());
  // Git scope discovery now yields; wait for the observable persistence boundary.
  await vi.waitFor(() => assert.equal(sent.length, 3));
  assert.match(String(sent[2]?.content), /new recipient/);
  await handlers.get('session_shutdown')!({}, context());
});
