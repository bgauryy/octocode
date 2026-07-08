import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { runAwarenessToolOperation } from '../src/tool-operations.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function run(
  db: DatabaseSync,
  operation: Parameters<typeof runAwarenessToolOperation>[1],
  request: Record<string, unknown>,
  cwd: string,
  agentId = 'agent-a',
) {
  return runAwarenessToolOperation(db, operation, request, { cwd, agentId, sessionId: 'sess-test' });
}

describe('runAwarenessToolOperation', () => {
  it('covers memory, reflection, refinement, query, wiki, digest, and harness operations', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-tool-ops-'));
    try {
      const db = freshDb();
      const recorded = run(db, 'record', {
        task_context: 'auth migration',
        observation: 'Run schema before data backfill',
        label: 'GOTCHA',
        tags: ['auth'],
        files: ['src/auth.ts'],
        workspace_path: dir,
      }, dir);
      expect(recorded.exitCode).toBe(0);
      const memoryId = (recorded.payload as { memory_id: string }).memory_id;

      const duplicate = run(db, 'record', {
        task_context: 'auth migration',
        observation: 'Run schema before data backfill',
        label: 'GOTCHA',
        workspace_path: dir,
      }, dir);
      expect(duplicate.exitCode).toBe(0);

      const recall = run(db, 'recall', { query: 'schema backfill', smart: true, files: ['src/auth.ts'] }, dir);
      expect((recall.payload as { count: number }).count).toBeGreaterThanOrEqual(1);

      const reflected = run(db, 'reflect', {
        task: 'auth migration',
        outcome: 'failed',
        lesson: 'Auth migrations need schema first',
        failure_signature: 'mechanism:auth|cause:order',
        fix_repo: 'Document auth migration order',
        fix_harness: 'Remind agents to check migration order',
        eval_failures: [{ id: 'eval-auth', failure_signature: 'mechanism:auth|cause:order' }],
        workspace_path: dir,
      }, dir);
      expect(reflected.exitCode).toBe(0);
      expect(reflected.payload).toMatchObject({ outcome: 'failed' });

      const refinements = run(db, 'refine_get', { workspace_path: dir, include_handoffs: true }, dir);
      expect((refinements.payload as { count: number }).count).toBeGreaterThanOrEqual(1);

      const weakness = run(db, 'mine_weakness', { workspace_path: dir, min_count: 1 }, dir);
      expect((weakness.payload as { total_memories: number }).total_memories).toBeGreaterThanOrEqual(1);

      const harness = run(db, 'export_harness', { workspace_path: dir, min_importance: 1, limit: 5 }, dir);
      expect(String((harness.payload as { markdown: string }).markdown)).toContain('auth');
      const emptyHarness = run(freshDb(), 'export_harness', { workspace_path: dir, min_importance: 10, limit: 1 }, dir);
      expect(String((emptyHarness.payload as { next: string }).next)).toContain('No harness proposals');

      const query = run(db, 'query', { view: 'all', workspace_path: dir, limit: 10 }, dir);
      expect((query.payload as { view: string }).view).toBe('all');

      const htmlPath = join(dir, 'awareness.html');
      const view = run(db, 'view', { view: 'all', workspace_path: dir, out: htmlPath }, dir);
      expect(view.payload).toMatchObject({ ok: true, path: htmlPath });
      expect(existsSync(htmlPath)).toBe(true);

      const injected = run(db, 'repo_inject', { workspace_path: dir, out_dir: join(dir, '.octocode'), mode: 'local', check: false }, dir);
      expect((injected.payload as { files: string[] }).files.some(file => file.endsWith('AGENTS.md'))).toBe(true);

      const forgotten = run(db, 'forget', { memory_id: memoryId, dry_run: true, workspace_path: dir }, dir);
      expect(forgotten.exitCode).toBe(0);

      const digest = run(db, 'digest', { dry_run: true, export_doc: true, workspace_path: dir }, dir);
      expect(digest.payload).toMatchObject({ dry_run: true });
      const nonDryDigest = run(db, 'digest', { retention_days: 1 }, dir);
      expect(nonDryDigest.payload).toHaveProperty('fts_rebuilt');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('covers lock, verify, signal, notify, and workspace status operations', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-tool-locks-'));
    try {
      const db = freshDb();
      const file = join(dir, 'src', 'a.ts');
      const lock = run(db, 'file_lock', {
        type: 'lock',
        target_files: [file],
        reasoning: 'edit file',
        ttl_ms: 60_000,
      }, dir);
      expect(lock.exitCode).toBe(0);
      const taskId = (lock.payload as { taskId: string }).taskId;
      expect(taskId).toMatch(/^task_/);

      const status = run(db, 'file_lock', { type: 'status', target_files: [file] }, dir);
      expect(status.exitCode).toBe(0);

      const conflict = run(db, 'file_lock', {
        type: 'lock',
        target_files: [file],
        reasoning: 'other edit',
      }, dir, 'agent-b');
      expect(conflict.exitCode).toBe(2);

      const pending = run(db, 'file_lock', {
        type: 'release',
        task_id: taskId,
        status: 'PENDING',
      }, dir);
      expect(pending.exitCode).toBe(0);

      const audit = run(db, 'audit_unverified', {}, dir);
      expect(audit.exitCode).toBe(1);
      expect((audit.payload as { count: number }).count).toBe(1);

      const verified = run(db, 'verify', { allPending: true, status: 'SUCCESS' }, dir);
      expect(verified.exitCode).toBe(0);

      const first = run(db, 'file_lock', { type: 'lock', target_files: [join(dir, 'b.ts')], reasoning: 'batch 1' }, dir);
      const second = run(db, 'file_lock', { type: 'lock', target_files: [join(dir, 'c.ts')], reasoning: 'batch 2' }, dir);
      const firstTask = (first.payload as { taskId: string }).taskId;
      const secondTask = (second.payload as { taskId: string }).taskId;
      run(db, 'file_lock', { type: 'release', task_id: firstTask, status: 'PENDING' }, dir);
      run(db, 'file_lock', { type: 'release', task_id: secondTask, status: 'PENDING' }, dir);
      const batch = run(db, 'verify', { task_ids: [firstTask, secondTask, firstTask], status: 'FAILED' }, dir);
      expect(batch.payload).toMatchObject({ count: 2 });

      const third = run(db, 'file_lock', { type: 'lock', target_files: [join(dir, 'd.ts')], reasoning: 'mixed pending' }, dir);
      const fourth = run(db, 'file_lock', { type: 'lock', target_files: [join(dir, 'e.ts')], reasoning: 'mixed pending two' }, dir);
      const thirdTask = (third.payload as { taskId: string }).taskId;
      const fourthTask = (fourth.payload as { taskId: string }).taskId;
      run(db, 'file_lock', { type: 'release', task_id: thirdTask, status: 'PENDING' }, dir);
      run(db, 'file_lock', { type: 'release', task_id: fourthTask, status: 'PENDING' }, dir);
      const mixed = run(db, 'verify', { task_id: thirdTask, allPending: true, status: 'SUCCESS' }, dir);
      expect(mixed.exitCode).toBe(0);

      const published = run(db, 'agent_signal', {
        action: 'publish',
        kind: 'question',
        subject: 'Need review',
        body: 'Please review the lock flow',
        to_agents: ['agent-b'],
        files: [file],
        refs: ['task:test'],
      }, dir);
      expect(published.exitCode).toBe(0);
      const signalId = (published.payload as { signal_id: string }).signal_id;

      const inbox = run(db, 'agent_signal', { action: 'list', agent_id: 'agent-b', mark_read: true }, dir);
      expect((inbox.payload as { count: number }).count).toBeGreaterThanOrEqual(1);

      const reply = run(db, 'agent_signal', {
        action: 'reply',
        in_reply_to: signalId,
        subject: 'Reviewed',
        body: 'Reviewed',
        to_agent: 'agent-a',
      }, dir, 'agent-b');
      expect(reply.exitCode).toBe(0);

      const ack = run(db, 'agent_signal', { action: 'ack', signal_ids: [signalId], agent_id: 'agent-b' }, dir);
      expect(ack.exitCode).toBe(0);

      const resolved = run(db, 'agent_signal', { action: 'resolve', signal_ids: [signalId] }, dir);
      expect(resolved.exitCode).toBe(0);

      const notify = run(db, 'notify', {
        kind: 'fyi',
        subject: 'legacy alias still works',
        body: 'Use agent_signal going forward',
        to_agent: 'agent-b',
      }, dir);
      expect(notify.payload).toMatchObject({ alias: 'memory_notify' });

      const workspace = run(db, 'workspace_status', { workspace_path: dir }, dir);
      expect(workspace.exitCode).toBe(0);
      expect(workspace.payload).toHaveProperty('active_tasks');

      const stale = run(db, 'file_lock', { type: 'lock', target_files: [join(dir, 'stale.ts')], reasoning: 'stale active' }, dir);
      const staleTask = (stale.payload as { taskId: string }).taskId;
      db.prepare('DELETE FROM locks WHERE task_id = ?').run(staleTask);
      const staleAudit = run(db, 'audit_unverified', {}, dir);
      expect(staleAudit.exitCode).toBe(1);
      expect(staleAudit.payload).toHaveProperty('stale_active');

      expect(() => run(db, 'verify', {}, dir)).toThrow('memory_verify requires');
      expect(() => run(db, 'agent_signal', { action: 'bad' }, dir)).toThrow('agent_signal requires');
      expect(() => run(db, 'file_lock', { type: 'bad' }, dir)).toThrow('memory_file_lock requires');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
