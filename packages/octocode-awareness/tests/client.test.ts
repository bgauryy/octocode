import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  createAwarenessClient,
  type AwarenessExecutableCall,
} from '../src/client.js';
import {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
  listAwarenessOperationDescriptors,
} from '../src/schema/operation-catalog.js';
import { executeAwarenessCommand } from '../src/command-api.js';
import { executeAwarenessCli } from '../src/command-cli.js';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture(agentId = 'owner') {
  const workspace = mkdtempSync(join(tmpdir(), 'awareness-client-'));
  roots.push(workspace);
  return {
    workspace,
    agentId,
    database: join(workspace, 'awareness.sqlite3'),
    compact: true,
  };
}

describe('canonical Awareness client', () => {
  it('publishes one routine surface of five concepts and nineteen operations', () => {
    expect(AWARENESS_CONCEPTS).toEqual(['context', 'work', 'message', 'memory', 'history']);
    expect(ROUTINE_AWARENESS_OPERATIONS).toHaveLength(19);
    expect(new Set(ROUTINE_AWARENESS_OPERATIONS)).toHaveLength(19);
    expect(new Set(listAwarenessOperationDescriptors().map(row => row.concept)))
      .toEqual(new Set(AWARENESS_CONCEPTS));
    expect(listAwarenessOperationDescriptors().every(row => row.visibility === 'routine')).toBe(true);
    expect(ROUTINE_AWARENESS_OPERATIONS).not.toEqual(expect.arrayContaining([
      'admin.database.consolidate', 'memory.prune', 'history.recovery', 'context.hook-run',
    ]));
  });

  it('binds host context once and executes operations through one registry', async () => {
    const context = fixture();
    const client = createAwarenessClient(context);
    const sent = await client.execute({ operation: 'message.send', params: {
      kind: 'question', subject: 'Need input', body: 'Which path?', to_agent: ['peer'],
    } });
    expect(sent.exitCode, JSON.stringify(sent.payload)).toBe(0);
    const inbox = await createAwarenessClient({ ...context, agentId: 'peer' })
      .execute({ operation: 'message.list', params: { include_bodies: true } });
    expect(inbox.exitCode, JSON.stringify(inbox.payload)).toBe(0);
    expect(JSON.stringify(inbox.payload)).toContain('Which path?');
    expect(readFileSync(new URL('../src/client.ts', import.meta.url), 'utf8'))
      .not.toContain('executeAwarenessCommand');
    const cliSource = readFileSync(new URL('../src/command-cli.ts', import.meta.url), 'utf8');
    expect(cliSource).toContain('operationCallForLegacyCommand');
    expect(cliSource.indexOf('if (routineCall)')).toBeLessThan(cliSource.indexOf('return executeAwarenessCommand'));
  });

  it('returns a bounded orientation and a minimal not-modified response', async () => {
    const context = fixture();
    await executeAwarenessCommand({ command: 'agent register', params: { agent_name: 'Owner' } }, context);
    const client = createAwarenessClient(context);
    const first = await client.orient();
    expect(first).toMatchObject({
      unchanged: false,
      self: { actorId: 'owner' },
      peers: { items: [{ actorId: 'owner' }], partial: false },
      partial: false,
    });
    expect(first.revision).toMatch(/^o2\./);
    expect(Buffer.byteLength(JSON.stringify(first))).toBeLessThanOrEqual(1_500);

    const second = await client.orient({ if_revision: first.revision });
    expect(second).toEqual({ revision: first.revision, unchanged: true });
    expect(Buffer.byteLength(JSON.stringify(second))).toBeLessThanOrEqual(160);
  });

  it('checks the event high-water mark before reading orientation domains', async () => {
    const context = fixture();
    await executeAwarenessCommand({ command: 'agent register', params: { agent_name: 'Owner' } }, context);
    const client = createAwarenessClient(context);
    const first = await client.orient();
    const db = new (await import('node:sqlite')).DatabaseSync(context.database);
    db.prepare("UPDATE awareness_agents SET metadata_json = '{' WHERE agent_id = 'owner'").run();
    db.close();

    await expect(client.orient({ if_revision: first.revision })).resolves.toEqual({
      revision: first.revision,
      unchanged: true,
    });
  });

  it('invalidates the orientation revision after a canonical event write', async () => {
    const context = fixture();
    await executeAwarenessCommand({ command: 'agent register', params: { agent_name: 'Owner' } }, context);
    const client = createAwarenessClient(context);
    const first = await client.orient();
    const sent = await client.execute({
      operation: 'message.send',
      params: { kind: 'fyi', subject: 'Changed', to_agent: ['owner'] },
    });
    expect(sent.exitCode, JSON.stringify(sent.payload)).toBe(0);
    const changed = await client.orient({ if_revision: first.revision });
    expect(changed.unchanged).toBe(false);
    expect(changed.revision).not.toBe(first.revision);
  });

  it('records and consumes bounded host events through the client', async () => {
    const client = createAwarenessClient(fixture());
    const receipt = await client.recordHostEvent({
      eventType: 'host.tool-completed', retentionClass: 'operational',
      payload: { tool: 'test' }, source: 'harness', actorKind: 'tool',
    });
    expect(receipt.sequence).toBeGreaterThan(0);
    const page = await client.consumeEvents({ afterSequence: 0, limit: 1 });
    expect(page.events).toHaveLength(1);
    expect(page.events[0]).toMatchObject({ sequence: receipt.sequence, type: 'host.tool-completed', payload: { tool: 'test' } });
    expect(page.next).toBeNull();
  });

  it('bounds host-supplied overlap insights as advisory data', async () => {
    const context = fixture();
    const started = await createAwarenessClient({ ...context, agentId: 'peer' }).execute({
      operation: 'work.create',
      params: { kind: 'standalone', file: ['src/shared.ts'], rationale: 'peer edit', test_plan: 'test' },
    });
    expect(started.exitCode, JSON.stringify(started.payload)).toBe(0);
    const baseline = await createAwarenessClient(context).orient();
    if (baseline.unchanged) throw new Error('expected changed baseline orientation');
    const client = createAwarenessClient({
      ...context,
      insightProvider: {
        async suggest() {
          return Array.from({ length: 5 }, (_, index) => ({
            summary: `candidate ${index} ${'x'.repeat(300)}`,
            attribution: 'host-provider', confidence: 2,
          }));
        },
      },
    });
    const orientation = await client.orient();
    if (orientation.unchanged) throw new Error('expected changed orientation');
    expect(orientation.insights).toMatchObject({ advisory: true });
    expect(orientation.insights?.candidates).toHaveLength(3);
    expect(orientation.insights?.candidates[0]?.summary.length).toBeLessThanOrEqual(160);
    expect(orientation.insights?.candidates[0]?.confidence).toBe(1);
    expect(orientation.verification).toEqual(baseline.verification);
  });

  it('returns executable canonical continuations for bounded peer pages', async () => {
    const context = fixture();
    for (let i = 0; i < 4; i++) {
      await executeAwarenessCommand(
        { command: 'agent register', params: { agent_name: `Peer ${i} ${'N'.repeat(72)}` } },
        { ...context, agentId: `peer-${i}` },
      );
    }
    const client = createAwarenessClient(context);
    const first = await client.orient({ limit: 2 });
    expect(first).toMatchObject({ partial: true, peers: { partial: true } });
    expect(Buffer.byteLength(JSON.stringify(first))).toBeLessThanOrEqual(1_500);
    if (first.unchanged) throw new Error('expected a changed orientation');
    const continuation = first.next.find(next => next.operation === 'context.orient');
    expect(continuation).toEqual({ operation: 'context.orient', params: { limit: 2, offset: 2 } });
    const second = await client.execute(continuation!);
    expect(second.exitCode).toBe(0);
    expect((second.payload as { peers: { items: unknown[] } }).peers.items).toHaveLength(2);
  });

  it('preserves executable continuations for partial detail projections', async () => {
    const context = fixture();
    for (let index = 0; index < 5; index++) {
      const sent = await executeAwarenessCommand({ command: 'signal publish', params: {
        kind: 'fyi', subject: `detail ${index}`, to_agent: ['owner'],
      } }, { ...context, agentId: `peer-${index}` });
      expect(sent.exitCode, JSON.stringify(sent.payload)).toBe(0);
    }
    const client = createAwarenessClient(context);
    const orientation = await client.orient({ limit: 1 });
    if (orientation.unchanged) throw new Error('expected changed orientation');
    expect(orientation.partial).toBe(true);
    expect(orientation.next.length).toBeGreaterThan(0);
    for (const continuation of orientation.next) {
      expect(listAwarenessOperationDescriptors().some(row => row.operation === continuation.operation)).toBe(true);
    }
    const detailContinuation = orientation.next.find(next => next.operation !== 'context.orient');
    expect(detailContinuation).toBeDefined();
    const page = await client.execute(detailContinuation!);
    expect(page.exitCode, JSON.stringify(page.payload)).toBe(0);
  });

  it('returns an executable lower-limit retry instead of an unbounded payload', async () => {
    const context = fixture();
    writeFileSync(join(context.workspace, 'large.txt'), 'x'.repeat(20_000));
    const db = connectDb(context.database);
    try {
      await runAwarenessHistoryOperation(db, 'capture', {
        workspace: context.workspace, agent_id: context.agentId, phase: 'before',
        operation_id: 'budget-read', file: ['large.txt'],
      });
    } finally { db.close(); }
    const client = createAwarenessClient(context);
    let result = await client.execute({ operation: 'history.read', params: {
      operation_id: 'budget-read', file: 'large.txt', side: 'before', limit: 20_000,
    } });
    expect(result).toMatchObject({
      exitCode: 2,
      payload: {
        error_code: 'OUTPUT_BUDGET_EXCEEDED',
        next: { retry: { operation: 'history.read' } },
      },
    });
    const firstRetry = (result.payload as { next: { retry: AwarenessExecutableCall<'history.read'> } }).next.retry;
    expect(firstRetry.params?.limit).toBeGreaterThan(0);
    expect(firstRetry.params?.limit).toBeLessThan(20_000);
    expect(Buffer.byteLength(JSON.stringify(result.payload))).toBeLessThanOrEqual(12_000);
    for (let attempts = 0; result.exitCode === 2 && attempts < 4; attempts++) {
      const retry = (result.payload as { next: { retry: AwarenessExecutableCall } }).next.retry;
      result = await client.execute(retry);
    }
    expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
  });

  it('executes a canonical history continuation produced with legacy args', async () => {
    const context = fixture();
    writeFileSync(join(context.workspace, 'paged.txt'), 'abcdef');
    const db = connectDb(context.database);
    try {
      await runAwarenessHistoryOperation(db, 'capture', {
        workspace: context.workspace, agent_id: context.agentId, phase: 'before',
        operation_id: 'paged-read', file: ['paged.txt'],
      });
    } finally { db.close(); }
    const client = createAwarenessClient(context);
    const first = await client.execute({ operation: 'history.read', params: {
      operation_id: 'paged-read', file: 'paged.txt', side: 'before', limit: 2,
    } });
    expect(first.exitCode, JSON.stringify(first.payload)).toBe(0);
    const next = (first.payload as { next: AwarenessExecutableCall<'history.read'> }).next;
    expect(next).toEqual({
      operation: 'history.read',
      params: { operation_id: 'paged-read', file: 'paged.txt', side: 'before', offset: 2, limit: 2 },
    });
    const second = await client.execute(next);
    expect(second.exitCode, JSON.stringify(second.payload)).toBe(0);
    expect(second.payload).toMatchObject({
      offset: 2,
      content: Buffer.from('cd').toString('base64'),
    });
    const legacy = await createAwarenessClient(context, { continuationFormat: 'legacy' }).execute({
      operation: 'history.read', params: {
        operation_id: 'paged-read', file: 'paged.txt', side: 'before', limit: 2,
      },
    });
    expect(legacy.payload).toMatchObject({
      next: { command: 'history read', args: { operation_id: 'paged-read', offset: 2 } },
    });
    const cli = await executeAwarenessCli([
      'history', 'read', '--db', context.database, '--workspace', context.workspace,
      '--operation-id', 'paged-read', '--file', 'paged.txt', '--side', 'before', '--limit', '2', '--compact',
    ]);
    expect(cli.payload).toMatchObject({
      next: { command: 'history read', args: { operation_id: 'paged-read', offset: 2 } },
    });
  });
});
