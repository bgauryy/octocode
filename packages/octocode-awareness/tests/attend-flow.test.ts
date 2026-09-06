import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';

const cleanups: Array<() => void> = [];
afterEach(() => cleanups.splice(0).forEach(cleanup => cleanup()));

function fixture(durable = true) {
  const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'aw-flow-'));
  const path = join(workspace, 'selected database.sqlite3');
  const db = new DatabaseSync(durable ? path : ':memory:');
  initDb(db);
  cleanups.push(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  const pending = (owner = 'owner') => db.prepare(`INSERT INTO task_runs
    (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
    VALUES ('run_pending', 'WORK', ?, 'bounded work', 'observed check', 'PENDING', ?, 'flow', ?, ?)`)
    .run(owner, workspace, new Date().toISOString(), new Date().toISOString());
  return { db, workspace, path, pending };
}

describe('agent flow decisions', () => {
  it('preserves selected storage, scope and literal identity in read-first verification guidance', () => {
    const { db, workspace, path, pending } = fixture();
    const owner = "agent ' $(touch unsafe)";
    pending(owner);
    const result = attendAwareness(db, { workspacePath: workspace, artifact: 'flow', agentId: owner, compact: true });
    expect(result.next).toMatchObject({
      action: 'verify_owned_work', target: { run_id: 'run_pending' },
      command: { name: 'verify audit', args: ['--db', path, '--workspace', workspace, '--artifact', 'flow', '--agent-id', owner, '--compact'] },
    });
    expect(JSON.stringify(result.next)).not.toContain('verify mark');
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id='run_pending'").get()?.status).toBe('PENDING');
  });

  it('does not advertise a subprocess for an in-memory store', () => {
    const { db, workspace, pending } = fixture(false);
    pending();
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true });
    expect(result.next).toMatchObject({ action: 'verify_owned_work', target: { run_id: 'run_pending' } });
    expect(result.next).not.toHaveProperty('command');
  });

  it('continues without a poll or mutation when no prerequisite exists', () => {
    const { db, workspace } = fixture();
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true });
    expect(result.next).toMatchObject({ action: 'continue' });
    expect(result.next).not.toHaveProperty('command');
  });
});
