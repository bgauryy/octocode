import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';
import { createAwarenessClient } from '../src/client.js';
import { insertMemory } from '../src/memory-write.js';

const cleanups: Array<() => void> = [];
afterEach(() => cleanups.splice(0).forEach(cleanup => cleanup()));

function fixture() {
  const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'attend-evidence-'));
  const dbPath = join(workspace, 'awareness.sqlite3');
  const db = new DatabaseSync(dbPath);
  initDb(db);
  cleanups.push(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  return { db, dbPath, workspace };
}

describe('attention evidence and continuations', () => {
  it('distinguishes selected memory leads from bounded recall and executes continuation', async () => {
    const { db, dbPath, workspace } = fixture();
    for (let i = 0; i < 5; i++) {
      (await insertMemory(db, {
        agentId: 'peer',
        taskContext: `attention contract ${i}`,
        observation: `Inspect attention contract ${i} before editing.`,
        importance: 5,
        workspacePath: workspace,
        references: [`file:${join(workspace, `source-${i}.ts`)}`],
      }));
    }
    const params = { workspacePath: workspace, agentId: 'owner', query: 'attention contract',
      file: Array.from({ length: 5 }, (_, i) => `source-${i}.ts`), compact: true };
    const first = attendAwareness(db, params);
    const repeat = attendAwareness(db, { ...params, revision: first.revision });
    expect(first.evidence).toHaveLength(1);
    expect(first).toMatchObject({ partial: true, evidence_omitted_count: 4 });
    expect(first.next.continuations).toHaveLength(1);
    const continuation = first.next.continuations?.[0];
    expect(continuation).toMatchObject({ command: 'memory recall' });
    const client = createAwarenessClient({
      database: dbPath,
      workspace,
      agentId: 'owner',
    });
    const executed = await client.execute({ operation: 'memory.recall', params: continuation!.params });
    expect(executed.exitCode).toBe(0);
    expect(executed.payload).toMatchObject({ memories: expect.any(Array) });
    expect((executed.payload as { memories: unknown[] }).memories.length).toBeGreaterThanOrEqual(5);
    expect(repeat).toMatchObject({ unchanged: true, revision: first.revision });
  });

  it('preserves peer context across unchanged observations', () => {
    const { db, workspace } = fixture();
    const peerRun = 'run_peer';
    const now = new Date().toISOString();
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES (?, 'WORK', 'peer', ?, ?, 'ACTIVE', ?, ?, ?)`)
      .run(peerRun, 'shared parser work', 'run parser checks', workspace, now, now);
    db.prepare(`INSERT INTO run_files
      (run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`)
      .run(peerRun, join(workspace, 'src/shared-parser.ts'), now, now, new Date(Date.now() + 60_000).toISOString());
    const contextual = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true });
    const unchanged = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true, revision: contextual.revision });
    expect(contextual.next).toMatchObject({ action: 'inspect_overlap', target: { file: 'src/shared-parser.ts' } });
    expect(unchanged).toMatchObject({ unchanged: true, revision: contextual.revision });
  });

  it('reports bounded workboard debt with an executable page continuation', async () => {
    const { db, dbPath, workspace } = fixture();
    const now = new Date().toISOString();
    const insert = db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES (?, 'WORK', ?, ?, ?, 'PENDING', ?, ?, ?)`);
    insert.run('run-one', 'owner', 'first pending run', 'check one', workspace, now, now);
    insert.run('run-two', 'peer', 'second pending run', 'check two', workspace, now, now);
    const params = { workspacePath: workspace, agentId: 'owner', limit: 1, compact: true };
    const first = attendAwareness(db, params);
    const repeat = attendAwareness(db, { ...params, revision: first.revision });
    expect(first.counts?.Verify).toBe(2);
    expect(first.workboard.Verify).toHaveLength(1);
    expect(first.operational_state.coverage.omitted_rows).toBeGreaterThan(0);
    expect(first).toMatchObject({ partial: true, partial_reasons: expect.arrayContaining(['workboard']) });
    const continuation = first.next.continuations?.find(item => item.command === 'query workboard');
    expect(continuation).toBeDefined();
    const client = createAwarenessClient({
      database: dbPath,
      workspace,
      agentId: 'owner',
    });
    const executed = await client.execute({
      operation: 'work.list',
      params: { kind: 'workboard', ...continuation!.params },
    });
    expect(executed.exitCode).toBe(0);
    expect(executed.payload).toMatchObject({ rows: expect.any(Array) });
    expect((executed.payload as { rows: unknown[] }).rows.length).toBeGreaterThanOrEqual(2);
    expect(repeat).toMatchObject({ unchanged: true, partial: true });
    db.prepare('UPDATE task_runs SET rationale = ?, updated_at = ? WHERE run_id = ?')
      .run('changed hidden pending run', new Date().toISOString(), 'run-two');
    const changedHidden = attendAwareness(db, { ...params, revision: first.revision });
    expect(changedHidden.revision).not.toBe(first.revision);
    expect(changedHidden).toMatchObject({ unchanged: false });
  });
});
