import { afterEach, describe, expect, it, vi } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';
import { agentSignal } from '../src/notifications-signals.js';
import { insertMemory } from '../src/memory-write.js';
import { createAwarenessClient } from '../src/client.js';

const cleanups: Array<() => void> = [];
afterEach(() => { vi.restoreAllMocks(); cleanups.splice(0).forEach(cleanup => cleanup()); });
function fixture() {
  const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'attend-revision-'));
  const db = new DatabaseSync(':memory:');
  initDb(db);
  cleanups.push(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  const params = { workspacePath: workspace, agentId: 'owner', compact: true };
  const run = (id: string, owner = 'owner') => {
    const now = new Date().toISOString();
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES (?, 'WORK', ?, 'bounded work', 'observed check', 'PENDING', ?, ?, ?)`)
      .run(id, owner, workspace, now, now);
  };
  return { db, workspace, params, run };
}

describe('scoped attend revisions', () => {
  it('suppresses invisible score decay while preserving changed memory ranking and expiry', async () => {
    const { db, params, workspace } = fixture();
    const initial = Date.now();
    const clock = vi.spyOn(Date, 'now').mockReturnValue(initial);
    const record = async (name: string, age: number, halfLife: number) => {
      const { memoryId } = (await insertMemory(db, { agentId: 'reader', taskContext: `source contract ${name}`,
        observation: `Inspect source contract ${name} before editing.`, importance: 5, workspacePath: workspace,
        references: [`file:${join(workspace, 'source.ts')}`] }));
      db.prepare('UPDATE awareness_memories SET created_at = ?, updated_at = ?, decay_half_life_days = ? WHERE memory_id = ?')
        .run(new Date(initial - age).toISOString(), new Date(initial - age).toISOString(), halfLife, memoryId);
      return memoryId;
    };
    const shortLived = (await record('rapid', 0, 0.01));
    const durable = (await record('durable', 2 * 86400000, 1000));
    const query = { ...params, query: 'source contract' };
    const first = attendAwareness(db, query);
    expect(first.evidence[0]?.id).toBe(shortLived);
    clock.mockReturnValue(initial + 1);
    expect(attendAwareness(db, { ...query, revision: first.revision }).unchanged).toBe(true);
    clock.mockReturnValue(initial + 86400000);
    const reordered = attendAwareness(db, { ...query, revision: first.revision });
    expect(reordered.unchanged).toBe(false);
    if (reordered.unchanged) throw new Error('Expected reordered evidence');
    expect(reordered.evidence[0]?.id).toBe(durable);
    db.prepare('UPDATE awareness_memories SET valid_to = ? WHERE memory_id = ?').run(new Date(initial - 1000).toISOString(), durable);
    const expired = attendAwareness(db, { ...query, revision: reordered.revision });
    expect(expired.unchanged).toBe(false);
    if (expired.unchanged) throw new Error('Expected expired evidence to disappear');
    expect(expired.evidence[0]?.id).toBe(shortLived);
  });

  it('returns a revision and then an explicitly unchanged bounded observation', async () => {
    const { db, params } = fixture();
    const first = attendAwareness(db, params);
    expect(first.revision).toMatch(/^a1\.[a-f0-9]{64}\.[a-f0-9]{64}$/);
    // Cross the generated clock's second precision; derived timestamps must not
    // make unchanged observations look different.
    await new Promise(resolve => setTimeout(resolve, 1100));
    const next = attendAwareness(db, { ...params, revision: first.revision });
    expect(next).toMatchObject({ unchanged: true, revision: first.revision, advisory: true });
    expect(next).not.toHaveProperty('workboard');
    expect(next).not.toHaveProperty('evidence');
    expect(next.unchanged && next.unavailable).toContain('context');
    expect(Buffer.byteLength(JSON.stringify(next))).toBeLessThan(Buffer.byteLength(JSON.stringify(first)));
  });

  it('returns fresh full state when debt changes and retains outstanding next actions', () => {
    const { db, params, run } = fixture();
    const first = attendAwareness(db, params);
    run('run_new');
    const changed = attendAwareness(db, { ...params, revision: first.revision });
    expect(changed).toMatchObject({ unchanged: false, next: { action: 'verify_owned_work' } });
    expect(changed.revision).not.toBe(first.revision);
    const same = attendAwareness(db, { ...params, revision: changed.revision });
    expect(same).toMatchObject({ unchanged: true, next: { action: 'verify_owned_work' } });
  });

  it('surfaces a newly directed blocker instead of suppressing communication', () => {
    const { db, params, workspace } = fixture();
    const first = attendAwareness(db, params);
    agentSignal(db, { action: 'publish', agentId: 'peer', workspacePath: workspace,
      kind: 'blocker', subject: 'Dependency check required', body: 'Inspect the changed public contract.', toAgents: ['owner'] });
    const next = attendAwareness(db, { ...params, revision: first.revision });
    expect(next).toMatchObject({ unchanged: false, next: { action: 'inspect_inbox' } });
  });

  it('does not let malformed, foreign, or changed filter tokens suppress output', () => {
    const { db, params } = fixture();
    const first = attendAwareness(db, params);
    for (const revision of ['broken', 'a2.' + '0'.repeat(64) + '.' + '0'.repeat(64)]) {
      expect(attendAwareness(db, { ...params, revision })).toMatchObject({ unchanged: false, reset_reason: 'invalid_revision' });
    }
    for (const changed of [{ agentId: 'peer' }, { workspacePath: '/other' }, { query: 'x' },
      { file: 'a.ts' }, { limit: 2 }, { compact: false }, { includeBodies: true }, { artifact: 'pkg' }, { repo: 'repo' }, { ref: 'branch' }]) {
      expect(attendAwareness(db, { ...params, ...changed, revision: first.revision }))
        .toMatchObject({ unchanged: false, reset_reason: 'scope_changed' });
    }
    const other = fixture();
    expect(attendAwareness(other.db, { ...params, revision: first.revision }))
      .toMatchObject({ unchanged: false, reset_reason: 'scope_changed' });
  });

  it('normalizes equivalent file scopes without widening the snapshot', () => {
    const { db, workspace, params } = fixture();
    const first = attendAwareness(db, { ...params, file: ['a.ts', 'b.ts'] });
    expect(attendAwareness(db, { ...params, file: [join(workspace, 'b.ts'), './a.ts'], revision: first.revision }))
      .toMatchObject({ unchanged: true });
  });

  it('invalidates a revision when the database schema version changes', () => {
    const { db, params } = fixture();
    const first = attendAwareness(db, params);
    db.exec('CREATE INDEX revision_fixture ON task_runs(agent_id)');
    expect(attendAwareness(db, { ...params, revision: first.revision }))
      .toMatchObject({ unchanged: false, reset_reason: 'scope_changed' });
  });

  it('keeps a changed external-writer version explicit instead of accepting a torn snapshot', () => {
    const { db, params } = fixture();
    const first = attendAwareness(db, params);
    const prepare = db.prepare.bind(db);
    let reads = 0;
    vi.spyOn(db, 'prepare').mockImplementation(sql => {
      const statement = prepare(sql);
      if (sql === 'PRAGMA data_version') vi.spyOn(statement, 'get').mockImplementation(() => ({ data_version: ++reads }));
      return statement;
    });
    expect(attendAwareness(db, { ...params, revision: first.revision }))
      .toMatchObject({ unchanged: false, reset_reason: 'unstable_snapshot' });
  });

  it('keeps unchanged pages lean while detecting omitted debt covered by the snapshot', () => {
    const { db, params, run } = fixture();
    run('run_one'); run('run_two');
    const first = attendAwareness(db, { ...params, limit: 1 });
    const next = attendAwareness(db, { ...params, limit: 1, revision: first.revision });
    expect(next).toMatchObject({ unchanged: true, partial: true });
    expect(next.next.continuations).toBeDefined();
    db.prepare("UPDATE task_runs SET rationale = 'changed hidden debt' WHERE run_id = 'run_two'").run();
    expect(attendAwareness(db, { ...params, limit: 1, revision: next.revision }))
      .toMatchObject({ unchanged: false });
  });

  it('never reports unchanged when hidden debt exceeds the snapshot cap', () => {
    const { db, params, run } = fixture();
    for (let index = 0; index < 51; index++) run(`run_${index}`);
    const first = attendAwareness(db, { ...params, limit: 1 });
    expect(attendAwareness(db, { ...params, limit: 1, revision: first.revision }))
      .toMatchObject({ unchanged: false, reset_reason: 'partial_snapshot' });
  });

  it('detects lease expiry without a database write', async () => {
    const { db, params, run, workspace } = fixture();
    run('run_peer', 'peer');
    db.prepare("UPDATE task_runs SET status = 'ACTIVE' WHERE run_id = 'run_peer'").run();
    const now = new Date().toISOString();
    const expiry = new Date(Date.now() + 1100).toISOString();
    const path = join(workspace, 'a.ts');
    db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES ('run_peer', ?, 'EXPLICIT', ?, ?, ?)`).run(path, now, now, expiry);
    db.prepare(`INSERT INTO awareness_locks (lock_id, file_path, run_id, acquired_at, expires_at)
      VALUES ('lock_peer', ?, 'run_peer', ?, ?)`).run(path, now, expiry);
    const first = attendAwareness(db, { ...params, file: 'a.ts' });
    expect(first.next.action).toBe('inspect_lock');
    await new Promise(resolve => setTimeout(resolve, 2100));
    const after = attendAwareness(db, { ...params, file: 'a.ts', revision: first.revision });
    expect(after).toMatchObject({ unchanged: false });
    expect(after.next.action).not.toBe('inspect_lock');
  });

  it('executes revision continuation across fresh clients and persistent database reopen', async () => {
    const { workspace } = fixture();
    const database = join(workspace, 'persistent.sqlite3');
    const call = async (ifRevision?: string) => {
      const result = await createAwarenessClient({ database, workspace, agentId: 'owner' }).execute({
        operation: 'context.orient', params: ifRevision ? { if_revision: ifRevision } : {},
      });
      expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
      return result.payload as Record<string, any>;
    };
    const first = await call();
    expect(first.revision).toMatch(/^o2\.[a-f0-9]{64}$/);
    const next = await call(first.revision);
    expect(next).toMatchObject({ unchanged: true, revision: first.revision });
    expect(await call('invalid')).toMatchObject({ unchanged: false, revision: first.revision });
  });
});
