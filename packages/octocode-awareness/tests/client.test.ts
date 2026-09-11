import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  createAwarenessClient,
} from '../src/client.js';
import {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
  listAwarenessOperationDescriptors,
} from '../src/schema/operation-catalog.js';
import { executeAwarenessCommand } from '../src/command-api.js';

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
});
