import { afterEach, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createAwarenessClient } from '../src/client.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture(agentId = 'lead') {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-canonical-depth-')));
  roots.push(workspace);
  return {
    workspace,
    database: join(workspace, 'awareness.sqlite3'),
    agentId,
    sessionId: `session-${agentId}`,
  };
}

function record(value: unknown): Record<string, any> {
  expect(value).toBeTruthy();
  expect(typeof value).toBe('object');
  return value as Record<string, any>;
}

async function expectSuccess(result: Promise<{ exitCode: number; payload: unknown }>) {
  const resolved = await result;
  expect(resolved.exitCode, JSON.stringify(resolved.payload)).toBe(0);
  return record(resolved.payload);
}

describe('deep canonical work lifecycle', () => {
  it('uses plan membership, documents, statuses, dependencies, ready selection, and task claims', async () => {
    const context = fixture();
    const lead = createAwarenessClient(context);
    const worker = createAwarenessClient({ ...context, agentId: 'worker', sessionId: 'session-worker' });
    const plan = await expectSuccess(lead.execute({
      operation: 'work.create',
      params: { kind: 'plan', name: 'Canonical depth', objective: 'Exercise the full durable lifecycle', artifact: 'release' },
    }));
    const planId = String(plan.plan_id);
    const planDirectory = dirname(String(plan.document));
    mkdirSync(join(planDirectory, 'docs'), { recursive: true });
    writeFileSync(join(planDirectory, 'docs/design.md'), '# Design contract\n');

    expect(await expectSuccess(worker.execute({
      operation: 'work.update', params: { transition: 'join', plan_id: planId },
    }))).toMatchObject({ plan_id: planId, agent_id: 'worker' });
    expect(await expectSuccess(lead.execute({
      operation: 'work.update',
      params: { transition: 'document', plan_id: planId, path: 'docs/design.md', title: 'Design contract' },
    }))).toMatchObject({ plan_id: planId, path: 'docs/design.md', title: 'Design contract' });
    expect(await expectSuccess(lead.execute({
      operation: 'work.update', params: { transition: 'status', plan_id: planId, status: 'PAUSED' },
    }))).toMatchObject({ plan_id: planId, status: 'PAUSED' });
    expect(await expectSuccess(lead.execute({
      operation: 'work.update', params: { transition: 'status', plan_id: planId, status: 'ACTIVE' },
    }))).toMatchObject({ plan_id: planId, status: 'ACTIVE' });

    const blocker = await expectSuccess(lead.execute({
      operation: 'work.create',
      params: {
        kind: 'task', plan_id: planId, title: 'Foundation', path: ['src/foundation.ts'],
        reasoning: 'The dependent task needs this API.', acceptance: 'Foundation checks pass.', priority: 10,
      },
    }));
    const blockerId = String(blocker.task_id);
    const dependent = await expectSuccess(lead.execute({
      operation: 'work.create',
      params: {
        kind: 'task', plan_id: planId, title: 'Dependent', path: ['src/dependent.ts'],
        reasoning: 'Consumes the foundation API.', acceptance: 'Integration checks pass.', priority: 5,
        depends_on: [blockerId],
      },
    }));
    const dependentId = String(dependent.task_id);
    const added = await expectSuccess(lead.execute({
      operation: 'work.create',
      params: {
        kind: 'task', plan_id: planId, title: 'Added dependency', path: ['src/added.ts'],
        reasoning: 'Dependency is discovered after planning.', acceptance: 'Dependency is persisted.',
      },
    }));
    const addedId = String(added.task_id);
    expect(await expectSuccess(lead.execute({
      operation: 'work.depend', params: { task_id: addedId, depends_on: [blockerId] },
    }))).toMatchObject({ task_id: addedId, dependency_count: 1 });

    const ready = await expectSuccess(worker.execute({
      operation: 'work.list', params: { kind: 'ready', plan_id: planId, limit: 10 },
    }));
    expect(ready.tasks).toEqual([expect.objectContaining({ task_id: blockerId })]);
    const claim = await expectSuccess(worker.execute({
      operation: 'work.claim', params: { next: true, plan_id: planId, lease_minutes: 1, test_plan: 'focused foundation test' },
    }));
    expect(claim).toMatchObject({ task_id: blockerId, run_id: expect.any(String), status: 'ACTIVE' });
    const runId = String(claim.run_id);

    expect(await expectSuccess(worker.execute({
      operation: 'work.update',
      params: { transition: 'heartbeat', task_id: blockerId, run_id: runId, lease_minutes: 2 },
    }))).toMatchObject({ task_id: blockerId, run_id: runId, status: 'ACTIVE' });
    expect(await expectSuccess(worker.execute({
      operation: 'work.update',
      params: { transition: 'release', task_id: blockerId, run_id: runId, blocked_reason: 'Waiting on review' },
    }))).toMatchObject({ task_id: blockerId, status: 'BLOCKED' });
    expect(await expectSuccess(lead.execute({
      operation: 'work.update', params: { transition: 'retry', task_id: blockerId, message: 'Review arrived' },
    }))).toMatchObject({ task_id: blockerId, status: 'OPEN' });

    const reclaimed = await expectSuccess(worker.execute({
      operation: 'work.claim', params: { task_id: blockerId, test_plan: 'rerun foundation test' },
    }));
    const finalRunId = String(reclaimed.run_id);
    expect(await expectSuccess(worker.execute({
      operation: 'work.update',
      params: { transition: 'submit', task_id: blockerId, run_id: finalRunId, message: 'Ready for observed verification' },
    }))).toMatchObject({ task_id: blockerId, run_id: finalRunId, status: 'PENDING' });
    expect(await expectSuccess(worker.execute({
      operation: 'work.verify',
      params: { action: 'mark', run_id: [finalRunId], status: 'SUCCESS', message: 'vitest foundation passed' },
    }))).toMatchObject({ ok: true });

    const nowReady = await expectSuccess(worker.execute({
      operation: 'work.list', params: { kind: 'ready', plan_id: planId, limit: 10 },
    }));
    expect(nowReady.tasks).toEqual(expect.arrayContaining([
      expect.objectContaining({ task_id: dependentId }),
      expect.objectContaining({ task_id: addedId }),
    ]));
  });

  it('declares, inspects, refreshes, ends, audits, and batch-verifies standalone work', async () => {
    const context = fixture('operator');
    const client = createAwarenessClient(context);
    const first = await expectSuccess(client.execute({
      operation: 'work.create',
      params: {
        kind: 'standalone', file: ['src/a.ts', 'src/b.ts'], rationale: 'Refactor coupled files',
        test_plan: 'focused refactor tests', context_ref: 'rfc:canonical', artifact: 'release',
        ttl_seconds: 90, exclusive: false,
      },
    }));
    const firstRunId = String(first.run_id);
    const second = await expectSuccess(client.execute({
      operation: 'work.create',
      params: {
        kind: 'standalone', file: ['src/c.ts'], rationale: 'Add coverage', test_plan: 'coverage gate',
        ttl_minutes: 2, artifact: 'release',
      },
    }));
    const secondRunId = String(second.run_id);

    expect(await expectSuccess(client.execute({
      operation: 'work.show', params: { kind: 'presence', file: ['src/a.ts'], full: true, all: true },
    }))).toMatchObject({ files: [expect.objectContaining({ run_id: firstRunId, file_path: join(context.workspace, 'src/a.ts') })] });
    expect(await expectSuccess(client.execute({
      operation: 'work.update', params: { transition: 'touch', run_id: firstRunId, file: ['src/a.ts'], ttl_seconds: 120 },
    }))).toMatchObject({ run_id: firstRunId, file_count: 1 });
    expect(await expectSuccess(client.execute({
      operation: 'work.update', params: { transition: 'end', run_id: firstRunId, file: ['src/a.ts'] },
    }))).toMatchObject({ run_id: firstRunId, file_count: 1 });
    expect(await expectSuccess(client.execute({
      operation: 'work.update', params: { transition: 'end', run_id: firstRunId, file: ['src/b.ts'] },
    }))).toMatchObject({ run_id: firstRunId, file_count: 1, status: 'PENDING' });
    expect(await expectSuccess(client.execute({
      operation: 'work.protect',
      params: {
        action: 'acquire', run_id: secondRunId, target_file: ['src/protected.ts'],
        rationale: 'Sensitive generated file', test_plan: 'regenerate and compare', ttl_seconds: 60,
      },
    }))).toMatchObject({ ok: true, run: { run_id: secondRunId } });
    expect(await expectSuccess(client.execute({
      operation: 'work.protect', params: { action: 'release', run_id: secondRunId, status: 'FAILED' },
    }))).toMatchObject({ released: true });

    const audit = await client.execute({
      operation: 'work.verify', params: { action: 'audit', artifact: 'release', origin: ['WORK'], limit: 1, offset: 0 },
    });
    expect(audit.exitCode).toBe(1);
    expect(record(audit.payload)).toMatchObject({
      count: expect.any(Number), returned_count: 1,
      pagination: { offset: 0, limit: 1, has_more: expect.any(Boolean) },
    });
    const batch = await client.execute({
      operation: 'work.verify',
      params: {
        action: 'mark', run_id: [firstRunId, secondRunId], status: 'SUCCESS',
        message: 'vitest canonical standalone lifecycle passed',
      },
    });
    expect(batch.exitCode).toBe(1);
    expect(record(batch.payload)).toMatchObject({
      ok: false,
      error: expect.stringContaining('only PENDING runs can be verified'),
      results: [expect.objectContaining({ ok: true }), expect.objectContaining({ ok: false })],
    });
    expect(await expectSuccess(client.execute({
      operation: 'work.list', params: { kind: 'presence', all: true, full: true, limit: 10, offset: 0 },
    }))).toMatchObject({ files: expect.any(Array) });
  });
});
