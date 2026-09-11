import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAwarenessClient, type AwarenessExecutableCall } from '../src/client.js';
import { executeCanonicalRoute, executeContextOrient } from '../src/operation-executor.js';
import { connectDb } from '../src/db-runtime.js';
import { workspaceEventHighWater } from '../src/event-outbox.js';
import {
  listAwarenessOperationDescriptors,
} from '../src/schema/operation-catalog.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture(agentId = 'owner') {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-client-held-out-')));
  roots.push(workspace);
  return {
    workspace,
    agentId,
    sessionId: 'session-1',
    database: join(workspace, 'awareness.sqlite3'),
  };
}

function payloadRecord(value: unknown): Record<string, any> {
  expect(value).toBeTruthy();
  expect(typeof value).toBe('object');
  return value as Record<string, any>;
}

function entityId(payload: Record<string, any>, kind: 'plan' | 'task'): string {
  const value = payload[`${kind}_id`] ?? payload[kind]?.[`${kind}_id`];
  expect(value).toEqual(expect.any(String));
  return value;
}

function highWater(database: string, workspace: string): number {
  const db = connectDb(database);
  try { return workspaceEventHighWater(db, workspace); }
  finally { db.close(); }
}

describe('canonical Awareness client held-out behavior', () => {
  it('keeps host context immutable and reports unknown and invalid operations', async () => {
    const context = fixture();
    const client = createAwarenessClient(context);
    context.agentId = 'forged';

    expect(client.context.agentId).toBe('owner');
    expect(Object.isFrozen(client.context)).toBe(true);
    expect(client.operations()).toBe(listAwarenessOperationDescriptors());

    const unknown = await client.execute({ operation: 'noise.missing' } as unknown as AwarenessExecutableCall);
    expect(unknown).toEqual({
      exitCode: 1,
      payload: {
        ok: false,
        operation: 'noise.missing',
        error: 'Unknown Awareness operation: noise.missing',
      },
    });

    const invalid = await client.execute({
      operation: 'message.send',
      params: { kind: 'fyi', subject: 'forged', workspace: '/forged' },
    } as unknown as AwarenessExecutableCall<'message.send'>);
    expect(invalid.exitCode).toBe(1);
    expect(payloadRecord(invalid.payload)).toMatchObject({
      ok: false,
      operation: 'message.send',
      issues: expect.arrayContaining([expect.objectContaining({ path: expect.any(String) })]),
    });

    await expect(client.orient({ limit: 4 })).rejects.toThrow('context.orient failed');
  });

  it('propagates event defaults, filters replay, and rolls back invalid host events', async () => {
    const context = fixture();
    const client = createAwarenessClient(context);
    const first = await client.recordHostEvent({
      eventType: 'host.started',
      retentionClass: 'operational',
      payload: { ordinal: 1 },
      source: 'harness',
      actorKind: 'agent',
    });
    const second = await client.recordHostEvent({
      eventType: 'host.completed',
      retentionClass: 'audit',
      payload: { ordinal: 2 },
      source: 'harness',
      actorKind: 'agent',
      sessionId: 'explicit-session',
      createdAt: '2026-01-02T03:04:05.000Z',
    });

    const filtered = await client.consumeEvents({
      afterSequence: first.sequence,
      eventType: 'host.completed',
      retentionClass: 'audit',
      limit: 2,
    });
    expect(filtered.events).toHaveLength(1);
    expect(filtered.events[0]).toMatchObject({
      sequence: second.sequence,
      sessionId: 'explicit-session',
      createdAt: '2026-01-02T03:04:05.000Z',
    });

    await expect(client.recordHostEvent({
      eventType: '',
      retentionClass: 'operational',
      payload: {},
      source: 'harness',
      actorKind: 'agent',
    })).rejects.toThrow();
    await expect(client.consumeEvents({ afterSequence: first.sequence }))
      .resolves.toMatchObject({ events: [expect.objectContaining({ sequence: second.sequence })] });
  });

  it('preserves cancellation and structured history failures', async () => {
    const context = fixture();
    const controller = new AbortController();
    controller.abort(new DOMException('stop now', 'AbortError'));
    const cancelled = await createAwarenessClient({ ...context, signal: controller.signal }).execute({
      operation: 'work.list',
    });
    expect(cancelled).toMatchObject({ exitCode: 1, cancelled: true });

    const missing = await createAwarenessClient(context).execute({
      operation: 'history.read',
      params: { operation_id: 'missing', file: 'missing.ts', side: 'before' },
    });
    expect(missing.exitCode).toBe(1);
    expect(payloadRecord(missing.payload).error).toEqual(expect.objectContaining({
      code: expect.any(String),
      message: expect.any(String),
    }));
  });

  it('rejects direct host-binding conflicts before opening a route', async () => {
    const context = fixture();
    const result = await executeCanonicalRoute({
      command: 'query workboard',
      schema: {
        type: 'object',
        properties: { workspace: { type: 'string' } },
        additionalProperties: false,
      },
      handler: 'query',
      action: 'workboard',
      effect: 'read',
    }, { workspace: join(context.workspace, 'forged') }, context);
    expect(result).toMatchObject({
      exitCode: 1,
      payload: { ok: false, operation: 'query workboard', error: expect.stringContaining('workspace conflicts') },
    });
  });

  it('rolls back validation, nonzero, thrown-handler, and event-write failures', async () => {
    const context = fixture();
    const invalid = await executeCanonicalRoute({
      command: 'query workboard',
      schema: {
        type: 'object',
        properties: { workspace: { type: 'string' }, required_value: { type: 'string' } },
        required: ['required_value'],
        additionalProperties: false,
      },
      handler: 'query', action: 'workboard', effect: 'read',
    }, {}, context);
    expect(invalid).toMatchObject({ exitCode: 1, payload: { error: expect.stringContaining('Invalid parameters') } });

    const nonzero = await executeCanonicalRoute({
      command: 'work nonsense',
      schema: { type: 'object', properties: { workspace: { type: 'string' } }, additionalProperties: false },
      handler: 'work', action: 'nonsense', effect: 'coordination-write',
    }, {}, context);
    expect(nonzero.exitCode).toBe(1);

    const thrown = await executeCanonicalRoute({
      command: 'held-out work start failure',
      schema: { type: 'object', properties: { workspace: { type: 'string' } }, additionalProperties: false },
      handler: 'work', action: 'start', effect: 'coordination-write',
    }, {}, context);
    expect(thrown).toMatchObject({ exitCode: 1, payload: { error: expect.any(String) } });

    const eventFailure = await executeCanonicalRoute({
      command: 'query workboard',
      schema: { type: 'object', properties: { workspace: { type: 'string' } }, additionalProperties: false },
      handler: 'query', action: 'workboard', effect: 'workspace-write',
    }, {}, { ...context, agentId: '' });
    expect(eventFailure).toMatchObject({ exitCode: 1, payload: { error: expect.any(String) } });
  });

  it('validates direct orientation bounds before opening storage', async () => {
    const context = fixture();
    await expect(executeContextOrient(context, { limit: 0 })).rejects.toThrow('integer from 1 to 3');
    await expect(executeContextOrient(context, { offset: -1 })).rejects.toThrow('non-negative integer');
  });

  it('retries contended acquire and wait operations without leaking a transaction', async () => {
    const context = fixture();
    const owner = createAwarenessClient(context);
    const peer = createAwarenessClient({ ...context, agentId: 'peer' });
    const initialHighWater = highWater(context.database, context.workspace);
    const first = await owner.execute({
      operation: 'work.protect',
      params: {
        action: 'acquire', target_file: ['src/exclusive.ts'],
        rationale: 'held-out exclusive edit', test_plan: 'held-out checks',
      },
    });
    expect(first.exitCode, JSON.stringify(first.payload)).toBe(0);
    const releaseOwner = (async () => {
      await new Promise(resolve => setTimeout(resolve, 25));
      const result = await owner.execute({
        operation: 'work.protect', params: { action: 'release', target_file: ['src/exclusive.ts'] },
      });
      expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
    })();
    const acquired = await peer.execute({
      operation: 'work.protect',
      params: {
        action: 'acquire', target_file: ['src/exclusive.ts'],
        rationale: 'take over after release', test_plan: 'held-out checks',
        wait_seconds: 1, retry_interval: 1,
      },
    });
    await releaseOwner;
    expect(acquired.exitCode, JSON.stringify(acquired.payload)).toBe(0);
    const releasePeer = (async () => {
      await new Promise(resolve => setTimeout(resolve, 25));
      const result = await peer.execute({
        operation: 'work.protect', params: { action: 'release', target_file: ['src/exclusive.ts'] },
      });
      expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
    })();
    const waited = await owner.execute({
      operation: 'work.protect',
      params: { action: 'wait', target_file: ['src/exclusive.ts'], wait_seconds: 1, retry_interval: 1 },
    });
    await releasePeer;
    expect(waited.exitCode, JSON.stringify(waited.payload)).toBe(0);
    expect(highWater(context.database, context.workspace)).toBe(initialHighWater + 4);
  });

  it('routes plan, task, verification, protection, and memory through the canonical executor', async () => {
    const context = fixture();
    const client = createAwarenessClient(context);
    const createdPlan = await client.execute({
      operation: 'work.create',
      params: { kind: 'plan', name: 'Held out', objective: 'Exercise canonical routes' },
    });
    expect(createdPlan.exitCode, JSON.stringify(createdPlan.payload)).toBe(0);
    const planId = entityId(payloadRecord(createdPlan.payload), 'plan');

    expect((await client.execute({ operation: 'work.list', params: { kind: 'plan' } })).exitCode).toBe(0);
    expect((await client.execute({ operation: 'work.show', params: { kind: 'plan', plan_id: planId } })).exitCode).toBe(0);

    const createdTask = await client.execute({
      operation: 'work.create',
      params: {
        kind: 'task', plan_id: planId, title: 'Covered task', path: ['src/covered.ts'],
        reasoning: 'Exercise canonical task routes', acceptance: 'Held-out assertions pass',
      },
    });
    expect(createdTask.exitCode, JSON.stringify(createdTask.payload)).toBe(0);
    const taskId = entityId(payloadRecord(createdTask.payload), 'task');

    expect((await client.execute({ operation: 'work.list', params: { kind: 'task', plan_id: planId } })).exitCode).toBe(0);
    expect((await client.execute({ operation: 'work.show', params: { kind: 'task', task_id: taskId } })).exitCode).toBe(0);
    const claimed = await client.execute({ operation: 'work.claim', params: { task_id: taskId } });
    expect(claimed.exitCode, JSON.stringify(claimed.payload)).toBe(0);
    const runId = payloadRecord(claimed.payload).run_id ?? payloadRecord(claimed.payload).run?.run_id;
    expect(runId).toEqual(expect.any(String));
    const peer = createAwarenessClient({ ...context, agentId: 'peer' });
    const peerWork = await peer.execute({
      operation: 'work.create',
      params: { kind: 'standalone', file: ['src/covered.ts'], rationale: 'overlap', test_plan: 'peer checks' },
    });
    expect(peerWork.exitCode, JSON.stringify(peerWork.payload)).toBe(0);
    const handoff = await peer.execute({
      operation: 'message.send',
      params: { kind: 'handoff', subject: 'Handoff covered task', to_agent: ['owner'] },
    });
    expect(handoff.exitCode, JSON.stringify(handoff.payload)).toBe(0);
    const richOrientation = await client.orient();
    expect(richOrientation).toMatchObject({ unchanged: false });
    if (richOrientation.unchanged) throw new Error('expected rich orientation');
    expect(richOrientation.work.owned).toBeDefined();
    expect(richOrientation.continuation?.title).toContain('Handoff');

    const invalidInsights = await createAwarenessClient({
      ...context,
      insightProvider: { async suggest() {
        return [
          { summary: '', attribution: 'provider', confidence: 0.5 },
          { summary: 'missing attribution', attribution: '', confidence: 0.5 },
          { summary: 'bad confidence', attribution: 'provider', confidence: Number.NaN },
        ];
      } },
    }).orient();
    if (invalidInsights.unchanged) throw new Error('expected changed orientation');
    expect(invalidInsights.insights).toBeUndefined();
    const beforeHeartbeat = highWater(context.database, context.workspace);
    const heartbeat = await client.execute({
      operation: 'work.update', params: { transition: 'heartbeat', task_id: taskId, run_id: runId },
    });
    expect(heartbeat.exitCode, JSON.stringify(heartbeat.payload)).toBe(0);
    expect(highWater(context.database, context.workspace)).toBe(beforeHeartbeat + 1);
    const heartbeatDb = connectDb(context.database);
    try {
      expect(heartbeatDb.prepare(
        'SELECT event_type FROM event_outbox WHERE workspace_path = ? AND sequence > ? ORDER BY sequence',
      ).all(context.workspace, beforeHeartbeat)).toEqual([{ event_type: 'task.heartbeat' }]);
      expect(heartbeatDb.isTransaction).toBe(false);
    } finally { heartbeatDb.close(); }

    const beforeSubmit = highWater(context.database, context.workspace);
    const submitted = await client.execute({
      operation: 'work.update', params: { transition: 'submit', task_id: taskId, run_id: runId },
    });
    expect(submitted.exitCode, JSON.stringify(submitted.payload)).toBe(0);
    expect(highWater(context.database, context.workspace)).toBe(beforeSubmit + 1);

    const failed = await client.execute({
      operation: 'work.verify',
      params: { action: 'mark', run_id: [runId], status: 'FAILED', message: 'held-out failure' },
    });
    expect(failed.exitCode, JSON.stringify(failed.payload)).toBe(0);
    const beforeRetry = highWater(context.database, context.workspace);
    const retried = await client.execute({
      operation: 'work.update', params: { transition: 'retry', task_id: taskId },
    });
    expect(retried.exitCode, JSON.stringify(retried.payload)).toBe(0);
    expect(highWater(context.database, context.workspace)).toBe(beforeRetry + 1);

    const reclaimed = await client.execute({ operation: 'work.claim', params: { task_id: taskId } });
    expect(reclaimed.exitCode, JSON.stringify(reclaimed.payload)).toBe(0);
    const releaseRunId = payloadRecord(reclaimed.payload).run_id;
    const beforeRelease = highWater(context.database, context.workspace);
    const released = await client.execute({
      operation: 'work.update',
      params: { transition: 'release', task_id: taskId, run_id: releaseRunId },
    });
    expect(released.exitCode, JSON.stringify(released.payload)).toBe(0);
    expect(highWater(context.database, context.workspace)).toBe(beforeRelease + 1);
    expect((await client.execute({ operation: 'work.verify', params: { action: 'audit' } })).exitCode).toBe(0);
    expect((await client.execute({
      operation: 'work.protect', params: { action: 'wait', target_file: ['src/covered.ts'], wait_seconds: 0 },
    })).exitCode).toBe(0);

    const recorded = await client.execute({
      operation: 'memory.record',
      params: {
        task_context: 'Canonical client coverage', observation: 'The descriptor owns route execution',
        importance: 7, label: 'ARCHITECTURE', tag: ['held-out'],
      },
    });
    expect(recorded.exitCode, JSON.stringify(recorded.payload)).toBe(0);
    const recalled = await client.execute({ operation: 'memory.recall', params: { query: 'descriptor' } });
    expect(recalled.exitCode, JSON.stringify(recalled.payload)).toBe(0);
    expect(await client.execute({ operation: 'history.status' })).toMatchObject({ exitCode: 0 });
  });
});
