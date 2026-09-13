import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { afterEach, describe, expect, it } from 'vitest';
import { registerAgent } from '../src/agents.js';
import { createAwarenessClient } from '../src/client.js';
import { connectDb } from '../src/db-runtime.js';
import { initDb } from '../src/db-init.js';
import { signalExpiresAt } from '../src/message-lifecycle.js';
import { getAwarenessOperationDescriptor } from '../src/schema/operation-catalog.js';
import { agentRows } from '../src/repo-coordination.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));

describe('first-class agent lifecycle inspection', () => {
  it('lists workspace-linked and global identities through work.list kind agents', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-agents-'));
    roots.push(workspace);
    const database = join(workspace, 'awareness.sqlite3');
    const db = connectDb(database);
    initDb(db);
    registerAgent(db, { agentId: 'local-peer', agentName: 'Local', workspacePath: workspace });
    registerAgent(db, { agentId: 'global-peer', agentName: 'Global' });
    registerAgent(db, { agentId: 'other-peer', agentName: 'Other', workspacePath: join(workspace, 'other') });
    db.close();

    const client = createAwarenessClient({ database, workspace, agentId: 'viewer' });
    const result = await client.execute({ operation: 'work.list', params: { kind: 'agents', limit: 20 } });

    expect(result.exitCode).toBe(0);
    expect(result.payload).toMatchObject({ view: 'agents', count: 2 });
    expect((result.payload as { rows: Array<{ agent_id: string }> }).rows.map(row => row.agent_id).sort()).toEqual([
      'global-peer', 'local-peer',
    ]);

    const limited = await client.execute({ operation: 'work.list', params: { kind: 'agents', limit: 1 } });
    expect(limited.payload).toMatchObject({
      is_partial: true,
      partial: true,
      next: { list: { operation: 'work.list', params: { kind: 'agents', limit: 1, offset: 1 } } },
    });
    const next = (limited.payload as { next: { list: { params: Record<string, unknown> } } }).next.list.params;
    expect(() => getAwarenessOperationDescriptor('work.list')!.validate(next)).not.toThrow();
  });

  it('publishes agents as a direct work.list kind with no caller-controlled workspace', () => {
    const descriptor = getAwarenessOperationDescriptor('work.list')!;
    expect(() => descriptor.validate({ kind: 'agents', limit: 20, offset: 40 })).not.toThrow();
    expect(() => descriptor.validate({ kind: 'agents', offset: -1 })).toThrow();
    expect(() => descriptor.validate({ kind: 'agents', workspace: '/forged' })).toThrow();
    expect(JSON.stringify(descriptor.inputSchema)).toContain('"const":"agents"');
  });

  it('observes message actors and addressed recipients without mutating the registry', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-observed-agents-'));
    roots.push(workspace);
    const database = join(workspace, 'awareness.sqlite3');
    const db = connectDb(database);
    initDb(db);
    db.close();

    const alpha = createAwarenessClient({ database, workspace, agentId: 'alpha' });
    expect((await alpha.execute({
      operation: 'message.send', params: { kind: 'fyi', subject: 'Alpha to beta', to_agent: ['beta'] },
    })).exitCode).toBe(0);

    const observedDb = connectDb(database);
    const insertObserved = observedDb.prepare(`INSERT INTO signals(
      signal_id, workspace_path, from_agent, to_agent, kind, subject, files_json, refs_json,
      thread_id, importance, status, created_at, expires_at
    ) VALUES (?, ?, ?, ?, 'fyi', 'Observed', '[]', '[]', ?, 5, 'open', ?, ?)`)
    insertObserved.run('signal-observed', workspace, 'relay', '["gamma",7,""]', 'signal-observed', '2026-09-13T00:00:00Z', signalExpiresAt('fyi', '2026-09-13T00:00:00Z'));
    insertObserved.run('signal-json-scalar', workspace, 'relay', '"delta"', 'signal-json-scalar', '2026-09-12T00:00:00Z', signalExpiresAt('fyi', '2026-09-12T00:00:00Z'));
    insertObserved.run('signal-malformed', workspace, 'relay', '["ghost"', 'signal-malformed', '2026-09-11T00:00:00Z', signalExpiresAt('fyi', '2026-09-11T00:00:00Z'));
    observedDb.close();

    const client = createAwarenessClient({ database, workspace, agentId: 'viewer' });
    expect((await client.execute({ operation: 'context.orient' })).exitCode).toBe(0);
    const result = await client.execute({ operation: 'work.list', params: { kind: 'agents', limit: 20 } });
    expect(result.exitCode).toBe(0);
    expect((result.payload as { rows: Array<{ agent_id: string; provenance: string }> }).rows)
      .toEqual(expect.arrayContaining([
        expect.objectContaining({ agent_id: 'alpha', provenance: 'observed' }),
        expect.objectContaining({ agent_id: 'beta', provenance: 'observed' }),
        expect.objectContaining({ agent_id: 'relay', provenance: 'observed' }),
        expect.objectContaining({ agent_id: 'gamma', provenance: 'observed' }),
        expect.objectContaining({ agent_id: 'delta', provenance: 'observed' }),
      ]));
    expect((result.payload as { rows: Array<{ agent_id: string }> }).rows)
      .not.toContainEqual(expect.objectContaining({ agent_id: '["ghost"' }));

    const filtered = await client.execute({ operation: 'work.list', params: { kind: 'agents', query: 'GAM' } });
    expect((filtered.payload as { rows: Array<{ agent_id: string }> }).rows.map(row => row.agent_id)).toEqual(['gamma']);

    const after = connectDb(database);
    expect((after.prepare('SELECT COUNT(*) AS count FROM awareness_agents').get() as { count: number }).count).toBe(0);
    after.close();
  });

  it('returns lossless ordered pages after registered and observed identities are deduplicated', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-agent-pages-'));
    roots.push(workspace);
    const database = join(workspace, 'awareness.sqlite3');
    const db = connectDb(database);
    initDb(db);
    registerAgent(db, { agentId: 'registered-a', workspacePath: workspace });
    registerAgent(db, { agentId: 'shared', workspacePath: workspace });
    db.close();

    const sender = createAwarenessClient({ database, workspace, agentId: 'observed-a' });
    expect((await sender.execute({
      operation: 'message.send', params: { kind: 'fyi', subject: 'Paged actors', to_agent: ['observed-b', 'shared'] },
    })).exitCode).toBe(0);

    const client = createAwarenessClient({ database, workspace, agentId: 'viewer' });
    const complete = await client.execute({ operation: 'work.list', params: { kind: 'agents', limit: 20 } });
    const expectedIds = (complete.payload as { rows: Array<{ agent_id: string }> }).rows.map(row => row.agent_id);
    expect(new Set(expectedIds)).toEqual(new Set(['observed-a', 'observed-b', 'shared', 'registered-a']));
    expect((complete.payload as { rows: Array<{ agent_id: string; provenance: string }> }).rows)
      .toContainEqual(expect.objectContaining({ agent_id: 'shared', provenance: 'registered' }));

    const ids: string[] = [];
    let params: Record<string, unknown> = { kind: 'agents', limit: 2 };
    let pages = 0;
    for (;;) {
      const result = await client.execute({ operation: 'work.list', params: params as never });
      expect(result.exitCode).toBe(0);
      const payload = result.payload as {
        rows: Array<{ agent_id: string }>;
        partial: boolean;
        next?: { list: { operation: 'work.list'; params: Record<string, unknown> } };
      };
      expect(payload.rows.length).toBeLessThanOrEqual(2);
      ids.push(...payload.rows.map(row => row.agent_id));
      pages += 1;
      if (!payload.next) {
        expect(payload.partial).toBe(false);
        break;
      }
      expect(payload.partial).toBe(true);
      expect(payload.next.list.operation).toBe('work.list');
      expect(payload.next.list.params).toMatchObject({ kind: 'agents', limit: 2, offset: pages * 2 });
      expect(() => getAwarenessOperationDescriptor('work.list')!.validate(payload.next!.list.params)).not.toThrow();
      params = payload.next.list.params;
    }

    expect(pages).toBeGreaterThan(1);
    expect(ids).toHaveLength(new Set(ids).size);
    expect(ids).toEqual(expectedIds);
  });

  it('bounds the merged projection in the SQLite statement', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-agent-sql-bound-'));
    roots.push(workspace);
    const database = join(workspace, 'awareness.sqlite3');
    const db = connectDb(database);
    initDb(db);
    registerAgent(db, { agentId: 'registered', workspacePath: workspace });

    const preparedSql: string[] = [];
    const tracedDb = new Proxy(db, {
      get(target, property) {
        if (property === 'prepare') return (sql: string) => {
          preparedSql.push(sql);
          return target.prepare(sql);
        };
        const value = Reflect.get(target, property);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    });

    expect(agentRows(tracedDb, { workspacePath: workspace, limit: 2, offset: 1 })).toEqual([]);
    expect(preparedSql).toHaveLength(1);
    expect(preparedSql[0]).toMatch(/LIMIT \? OFFSET \?\s*$/);
    db.close();
  });
});
