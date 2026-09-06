import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { Worker } from 'node:worker_threads';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let dbPath: string;
let aw: AwarenessStore;

function projectionWorker(input: {
  sourcePlanKey: string;
  title: string;
  rfcRevision: string;
  goal: string;
  agentId: string;
  steps: Array<{ sourceStepKey: string; title: string; paths: string[]; reasoning: string; acceptance: string; dependsOnStepKeys?: string[] }>;
}) {
  const source = `
    import { parentPort, workerData } from 'node:worker_threads';
    const { openAwarenessStore } = await import(workerData.moduleUrl);
    const store = openAwarenessStore({ workspace: workerData.workspace, dbPath: workerData.dbPath });
    parentPort.postMessage({ type: 'ready' });
    parentPort.once('message', () => {
      try {
        const graph = store.materializePlanGraph(workerData.input);
        parentPort.postMessage({
          type: 'result', outcome: 'success', planId: graph.plan.planId,
          taskIds: [...graph.tasks.entries()].map(([stepKey, task]) => [stepKey, task.taskId]),
        });
      } catch (error) {
        parentPort.postMessage({ type: 'result', outcome: 'rejected', message: error instanceof Error ? error.message : String(error) });
      } finally {
        store.close();
      }
    });
  `;
  const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(source)}`), {
    workerData: {
      moduleUrl: new URL('../../out/index.js', import.meta.url).href,
      workspace,
      dbPath,
      input,
    },
  });
  let readyResolve!: () => void;
  let resultResolve!: (result: { outcome: 'success' | 'rejected'; planId?: string; taskIds?: Array<[string, string]>; message?: string }) => void;
  let reject!: (error: Error) => void;
  const ready = new Promise<void>((resolve, rejectPromise) => { readyResolve = resolve; reject = rejectPromise; });
  const result = new Promise<{ outcome: 'success' | 'rejected'; planId?: string; taskIds?: Array<[string, string]>; message?: string }>((resolve) => { resultResolve = resolve; });
  worker.on('message', (message: { type: string; outcome?: 'success' | 'rejected'; planId?: string; taskIds?: Array<[string, string]>; message?: string }) => {
    if (message.type === 'ready') readyResolve();
    else if (message.type === 'result' && message.outcome) resultResolve({
      outcome: message.outcome,
      ...(message.planId ? { planId: message.planId } : {}),
      ...(message.taskIds ? { taskIds: message.taskIds } : {}),
      ...(message.message ? { message: message.message } : {}),
    });
  });
  worker.once('error', reject);
  return { worker, ready, result };
}

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-plan-race-'));
  dbPath = join(workspace, '.octocode', 'awareness.sqlite3');
  aw = openAwarenessStore({ workspace, dbPath });
});

afterEach(async () => {
  aw.close();
  await rm(workspace, { recursive: true, force: true });
});

describe('plan graph concurrency and transactional faults', () => {
  it('serializes competing independent-store projections onto one stable graph', async () => {
    const input = {
      sourcePlanKey: 'competing-start', title: 'Competing Start', rfcRevision: 'sha256:accepted',
      goal: 'Serialize canonical source graph projection.', agentId: 'lead',
      steps: [
        { sourceStepKey: 'one', title: 'One', paths: ['src/one.ts'], reasoning: 'One rationale', acceptance: 'One acceptance' },
        { sourceStepKey: 'two', title: 'Two', paths: ['src/two.ts'], reasoning: 'Two rationale', acceptance: 'Two acceptance', dependsOnStepKeys: ['one'] },
      ],
    };
    const workers = Array.from({ length: 8 }, () => projectionWorker(input));
    await Promise.all(workers.map(({ ready }) => ready));
    for (const { worker } of workers) worker.postMessage('start');
    const results = await Promise.all(workers.map(({ result }) => result));

    expect(results.map((result) => result.outcome)).toEqual(Array(8).fill('success'));
    expect(new Set(results.map((result) => result.planId))).toHaveLength(1);
    expect(new Set(results.map((result) => JSON.stringify(result.taskIds)))).toHaveLength(1);
    const plans = aw.listPlans();
    expect(plans).toHaveLength(1);
    expect(aw.listTasks({ planId: plans[0]!.planId })).toHaveLength(2);
    expect(aw.listEvents({ consumerId: 'competing-projection', limit: 20 })
      .filter((event) => event.type === 'plan.projected')).toHaveLength(1);
  });

  it('rolls back the complete graph when the transactional outbox insert fails', () => {
    const db = new DatabaseSync(aw.dbPath);
    try {
      db.exec(`CREATE TRIGGER fail_plan_projection
        BEFORE INSERT ON event_outbox
        WHEN NEW.event_type = 'plan.projected'
        BEGIN SELECT RAISE(ABORT, 'injected projection event failure'); END`);
    } finally {
      db.close();
    }

    expect(() => aw.materializePlanGraph({
      sourcePlanKey: 'faulted-projection', title: 'Faulted projection', rfcRevision: 'sha256:fault',
      goal: 'Verify graph rollback.', agentId: 'lead',
      steps: [
        { sourceStepKey: 's1', title: 'One', paths: ['src/one.ts'], reasoning: 'One rationale', acceptance: 'One acceptance' },
        { sourceStepKey: 's2', title: 'Two', paths: ['src/two.ts'], reasoning: 'Two rationale', acceptance: 'Two acceptance' },
      ],
    })).toThrow(/injected projection event failure/);
    expect(aw.getPlanBySourceKey({ sourceKind: 'local', sourceKey: 'faulted-projection' })).toBeNull();
    expect(aw.listPlans()).toHaveLength(0);
    expect(aw.listTasks({})).toHaveLength(0);
    expect(aw.listEvents({ consumerId: 'faulted-projection', limit: 20 })).toHaveLength(0);
  });
});
