import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';

const DIST_INDEX_URL = new URL('../../out/index.js', import.meta.url).href;

const ACQUIRE_LOCK = `
const [moduleUrl, workspace, dbPath, agentId, filePath, operation] = process.argv.slice(1);
const { openAwarenessStore } = await import(moduleUrl);
const store = openAwarenessStore({ workspace, dbPath });
// Widen the read/write window: without a write reservation every contender
// observes OPEN before any claimant commits.
if (operation === 'task') {
  const getTask = store.getTask.bind(store);
  store.getTask = (id) => {
    const task = getTask(id);
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 40);
    return task;
  };
}
process.send({ type: 'ready', agentId });
process.once('message', () => {
  let result;
  try {
    const lock = operation === 'task' ? store.claimTask({ taskId: filePath, agentId, leaseSeconds: 60 }) : store.acquireLock({ filePath, agentId, reason: 'concurrent regression', testPlan: 'process lock regression', ttlSeconds: 60 });
    result = { type: 'result', outcome: 'success', agentId, owner: lock.agentId };
  } catch (error) {
    result = {
      type: 'result',
      outcome: 'conflict',
      agentId,
      message: error instanceof Error ? error.message : String(error),
    };
  }
  process.send(result, () => {
    store.close();
    process.disconnect();
  });
});
`;

interface ChildResult {
  outcome: 'success' | 'conflict';
  agentId: string;
  owner?: string;
  message?: string;
}

function lockProcess(workspace: string, dbPath: string, agentId: string, filePath: string, operation: string) {
  const child = spawn(process.execPath, [
    '--input-type=module',
    '--eval',
    ACQUIRE_LOCK,
    DIST_INDEX_URL,
    workspace,
    dbPath,
    agentId,
    filePath,
    operation,
  ], { stdio: ['ignore', 'ignore', 'pipe', 'ipc'] });

  let stderr = '';
  child.stderr!.setEncoding('utf8');
  child.stderr!.on('data', (chunk: string) => { stderr += chunk; });

  let readyResolve!: () => void;
  let resultResolve!: (result: ChildResult) => void;
  let reject!: (error: Error) => void;
  const ready = new Promise<void>((resolve, rejectPromise) => {
    readyResolve = resolve;
    reject = rejectPromise;
  });
  const result = new Promise<ChildResult>((resolve) => { resultResolve = resolve; });
  const closed = new Promise<void>((resolve) => {
    child.once('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`lock child ${agentId} exited ${code}: ${stderr}`));
    });
  });

  child.on('message', (message: { type?: string } & Partial<ChildResult>) => {
    if (message.type === 'ready') readyResolve();
    if (message.type === 'result' && message.outcome && message.agentId) {
      resultResolve({
        outcome: message.outcome,
        agentId: message.agentId,
        ...(message.owner === undefined ? {} : { owner: message.owner }),
        ...(message.message === undefined ? {} : { message: message.message }),
      });
    }
  });
  child.once('error', reject);

  return {
    child: child as ChildProcess,
    ready,
    result,
    closed,
  };
}

describe('coordination lock process concurrency', () => {
  it.each(['lock', 'task'])('atomically grants one %s owner across competing processes', { timeout: 60_000 }, async (operation) => {
    const workspace = await mkdtemp(join(tmpdir(), 'aw-lock-process-race-'));
    const dbPath = join(workspace, 'awareness.sqlite3');
    let filePath = 'src/shared.ts';
    let contenders: ReturnType<typeof lockProcess>[] = [];

    try {
      const initialized = openAwarenessStore({ workspace, dbPath });
      if (operation === 'task') {
        const plan = initialized.createPlan({ title: 'Concurrent claims', goal: 'Prove a single canonical claimant.', agentId: 'lead' });
        filePath = initialized.addTask({ planId: plan.planId, title: 'One owner', paths: ['src/shared.ts'], reasoning: 'Only one worker may hold the run claim.', acceptance: 'The claim is fenced.', agentId: 'lead' }).taskId;
      }
      initialized.close();

      contenders = Array.from({ length: 8 }, (_unused, index) => (
        lockProcess(workspace, dbPath, `agent-${index + 1}`, filePath, operation)
      ));
      await Promise.all(contenders.map(({ ready }) => ready));
      for (const { child } of contenders) child.send('acquire');

      const results = await Promise.all(contenders.map(({ result }) => result));
      await Promise.all(contenders.map(({ closed }) => closed));
      const winners = results.filter(({ outcome }) => outcome === 'success');
      const losers = results.filter(({ outcome }) => outcome === 'conflict');

      expect(winners).toHaveLength(1);
      expect(new Set(winners.map(({ agentId }) => agentId))).toHaveLength(1);
      expect(losers).toHaveLength(contenders.length - 1);
      expect(losers.every(({ message }) => message?.includes(operation === 'task' ? 'already claimed' : 'lock conflict'))).toBe(true);

      const stored = openAwarenessStore({ workspace, dbPath });
      try {
        expect(operation === 'task' ? [stored.getTask(filePath)] : stored.listLocks()).toEqual([
          expect.objectContaining({ agentId: winners[0]!.agentId }),
        ]);
      } finally {
        stored.close();
      }
    } finally {
      for (const { child } of contenders) {
        if (!child.killed && child.exitCode === null) child.kill();
      }
      await rm(workspace, { recursive: true, force: true });
    }
  });
});
