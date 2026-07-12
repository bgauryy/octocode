import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import * as awarenessApi from '../src/index.js';
import { connectCachedDb, evictExpiredLocks, ftsTermsForRow, getDb, initDb, rebuildFts, referenceKind, replaceMemoryReferences, resolveDbPath, tableColumns } from '../src/db.js';
import { insertHarnessLog, queryHarnessLog, sha256Hex } from '../src/audit.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { insertMemory, searchByEmbedding, storeEmbedding } from '../src/memory.js';
import { insertNotification, pruneNotifications } from '../src/notifications.js';
import { getPiAwarenessAgentId, getPiAwarenessSessionId } from '../src/pi-hooks.js';
import { deleteRefinement, getRefinements, insertRefinement, updateRefinement } from '../src/refinements.js';
import { endSession, getOrCreateSession, insertSession, listSessions } from '../src/sessions.js';
import { auditUnverified, markVerified } from '../src/verify.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('core branch coverage helpers', () => {
  it('exercises public barrel and DB utility branches', () => {
    expect(typeof awarenessApi.runAwarenessToolOperation).toBe('function');
    expect(() => getDb()).toThrow('Database not connected');

    const dir = mkdtempSync(join(tmpdir(), 'oc-db-utils-'));
    try {
      const dbPath = join(dir, 'awareness.sqlite3');
      const first = connectCachedDb(dbPath);
      const second = connectCachedDb(dbPath);
      expect(first).toBe(second);
      expect(resolveDbPath(dbPath)).toBe(dbPath);
      expect(tableColumns(first, 'memories')).toContain('memory_id');
      expect(referenceKind('https://octocode.ai')).toBe('url');
      expect(referenceKind('file:/tmp/a.ts')).toBe('file');
      expect(referenceKind('plain text')).toBe('other');
      expect(ftsTermsForRow({ tags_json: JSON.stringify(['tag-a']), label: 'GOTCHA', references: ['file:/tmp/a.ts'] })).toBe('tag-a gotcha file:/tmp/a.ts');

      const { memoryId } = insertMemory(first, {
        agentId: 'agent-a',
        taskContext: 'db utility memory',
        observation: 'replace refs then rebuild',
        importance: 5,
        label: 'OTHER',
      });
      replaceMemoryReferences(first, memoryId, ['https://octocode.ai', 'file:/tmp/a.ts', 'custom:value']);
      rebuildFts(first);
      const fts = first.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('custom') as { memory_id: string } | undefined;
      expect(fts?.memory_id).toBe(memoryId);
      const task = preFlightIntent(first, {
        agentId: 'agent-a',
        targetFiles: [join(dir, 'a.ts')],
        workspacePath: dir,
        ttlMs: 60_000,
      });
      if (!task.ok) throw new Error('claim failed');
      first.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(new Date(Date.now() - 1000).toISOString(), task.run.run_id);
      expect(evictExpiredLocks(first)).toEqual({ pruned_locks: 1 });
      expect(evictExpiredLocks(first)).toEqual({ pruned_locks: 0 });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('updates, filters, dry-runs, and deletes refinements', () => {
    const db = freshDb();
    const { refinementId } = insertRefinement(db, {
      agentId: 'agent-a',
      workspacePath: '/repo',
      artifact: 'svc',
      reasoning: 'old reasoning',
      remember: 'old note',
      quality: 'good',
      state: 'open',
      files: ['/repo/a.ts'],
    });

    expect(() => updateRefinement(db, { refinementId })).toThrow('no fields');
    const missing = updateRefinement(db, { refinementId: 'ref_missing', state: 'done' });
    expect(missing).toEqual({ updated: false, refinement: null });

    const updated = updateRefinement(db, {
      refinementId,
      state: 'ongoing',
      quality: 'bad',
      reasoning: 'new reasoning',
      remember: 'new note',
      files: ['/repo/b.ts'],
    });
    expect(updated.updated).toBe(true);
    expect(updated.refinement).toMatchObject({
      quality: 'bad',
      state: 'ongoing',
      reasoning: 'new reasoning',
      remember: 'new note',
      files: ['/repo/b.ts'],
    });

    expect(getRefinements(db, { workspacePath: '/repo', artifact: 'svc', states: ['ongoing'], quality: 'bad' }).count).toBe(1);
    expect(deleteRefinement(db, { refinementIds: [], workspacePath: '/repo' })).toEqual({ deleted: 0, refinement_ids: [] });
    const dry = deleteRefinement(db, { refinementIds: [refinementId], workspacePath: '/repo', artifact: 'svc', dryRun: true });
    expect(dry).toMatchObject({ deleted: 0, dry_run: true, would_delete: 1 });
    expect(deleteRefinement(db, { refinementIds: [refinementId], workspacePath: '/repo', artifact: 'svc' })).toMatchObject({ deleted: 1 });
  });

  it('records harness logs and filters sessions', () => {
    const db = freshDb();
    const session = insertSession(db, { agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc' });
    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-a',
      sessionId: session.session_id,
      workspacePath: '/repo',
      artifact: 'svc',
      eventType: 'capture',
      payload: { ok: true },
    });
    expect(harnessId).toMatch(/^harness_/);
    expect(sha256Hex('abc')).toHaveLength(64);
    expect(queryHarnessLog(db, { sessionId: session.session_id, agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc', eventType: 'capture', limit: 1 })).toHaveLength(1);

    const existing = getOrCreateSession(db, { agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc' });
    expect(existing).toBe(session.session_id);
    endSession(db, { sessionId: session.session_id, agentId: 'agent-a', summary: 'done' });
    const created = getOrCreateSession(db, { agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc' });
    expect(created).not.toBe(session.session_id);
    expect(listSessions(db, { agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc', active: true, limit: 5 })).toHaveLength(1);
  });

  it('fails pending and stale active verification work only by explicit run id', () => {
    const db = freshDb();
    const pending = preFlightIntent(db, {
      agentId: 'agent-a',
      workspacePath: '/repo',
      artifact: 'svc',
      targetFiles: ['/repo/a.ts'],
      rationale: 'pending task',
      testPlan: 'test pending',
    });
    if (!pending.ok) throw new Error('pending claim failed');
    releaseFileLock(db, { agentId: 'agent-a', runId: pending.run.run_id, status: 'PENDING' });

    const stale = preFlightIntent(db, {
      agentId: 'agent-a',
      workspacePath: '/repo',
      artifact: 'svc',
      targetFiles: ['/repo/stale.ts'],
      rationale: 'stale active task',
      testPlan: 'test stale',
      ttlMs: 60_000,
    });
    if (!stale.ok) throw new Error('stale claim failed');
    db.prepare('DELETE FROM locks WHERE run_id = ?').run(stale.run.run_id);
    db.prepare('UPDATE run_files SET expires_at = ? WHERE run_id = ?')
      .run('2000-01-01T00:00:00Z', stale.run.run_id);

    const audit = auditUnverified(db, { agentId: 'agent-a', workspacePath: '/repo', artifact: 'svc' });
    expect(audit.count).toBe(2);
    expect(audit.stale_active).toHaveLength(1);

    expect(markVerified(db, {
      runId: pending.run.run_id, agentId: 'agent-a', status: 'FAILED', message: 'pending work failed',
    })).toMatchObject({ ok: true, run_id: pending.run.run_id, status: 'FAILED' });
    expect(markVerified(db, {
      runId: stale.run.run_id, agentId: 'agent-a', status: 'FAILED', message: 'stale presence confirmed',
    })).toMatchObject({ ok: true, run_id: stale.run.run_id, status: 'FAILED' });
    expect(markVerified(db, { runId: '', agentId: 'agent-a' })).toMatchObject({ ok: false, run_id: null });
    expect(markVerified(db, { runId: 'task_missing', agentId: 'agent-a' })).toMatchObject({ ok: false });
  });

  it('stores and searches embeddings, including filtered and zero-vector cases', () => {
    const db = freshDb();
    const { memoryId: first } = insertMemory(db, {
      agentId: 'agent-a',
      taskContext: 'embedding one',
      observation: 'first vector',
      importance: 6,
      label: 'OTHER',
    });
    const { memoryId: second } = insertMemory(db, {
      agentId: 'agent-a',
      taskContext: 'embedding two',
      observation: 'second vector',
      importance: 6,
      label: 'OTHER',
    });
    const { memoryId: corrupt } = insertMemory(db, {
      agentId: 'agent-a',
      taskContext: 'embedding corrupt',
      observation: 'bad blob',
      importance: 6,
      label: 'OTHER',
    });
    storeEmbedding(db, first, new Float32Array([1, 0, 0]), 'model-a');
    storeEmbedding(db, second, new Float32Array([0, 1, 0]), 'model-b');
    db.prepare('UPDATE memories SET embedding = ?, embedding_model = ? WHERE memory_id = ?')
      .run(Buffer.from([1, 2, 3]), 'model-a', corrupt);

    expect(searchByEmbedding(db, new Float32Array([1, 0, 0]), 5, 0.5)[0]).toMatchObject({ memory_id: first });
    expect(searchByEmbedding(db, new Float32Array([1, 0, 0]), 5, 0.5, 'model-b')).toHaveLength(0);
    expect(searchByEmbedding(db, new Float32Array([0, 0, 0]), 5, 0.1)).toHaveLength(0);
    expect(searchByEmbedding(db, new Float32Array([1, 0]), 5, 0.1)).toHaveLength(0);
  });

  it('covers notification prune no-op and explicit resolved cleanup', () => {
    const db = freshDb();
    expect(() => pruneNotifications(db, { agentId: 'agent-b' })).toThrow(/resolved/);
    const signal = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'fyi',
      subject: 'resolved cleanup',
      workspacePath: '/repo',
    });
    db.prepare("UPDATE signals SET status = 'resolved', created_at = ? WHERE signal_id = ?")
      .run(new Date(Date.now() - 10 * 86400000).toISOString(), signal.signal_id);
    expect(pruneNotifications(db, { agentId: 'agent-b', resolvedOnly: true, olderThanDays: 1, workspacePath: '/repo', dryRun: true })).toMatchObject({
      deleted: 0,
      would_delete: 1,
    });
    expect(pruneNotifications(db, { agentId: 'agent-b', notificationIds: [signal.signal_id], resolvedOnly: true, olderThanDays: 1, workspacePath: '/repo' })).toMatchObject({ deleted: 1 });
  });


  it('derives Pi awareness ids from env, session files, and fallbacks', () => {
    const previous = process.env.OCTOCODE_AGENT_ID;
    delete process.env.OCTOCODE_AGENT_ID;
    const dir = mkdtempSync(join(tmpdir(), 'oc-pi-id-'));
    try {
      const ctx = { sessionManager: { getSessionFile: () => join(dir, 'session.jsonl') } };
      expect(getPiAwarenessSessionId(ctx)).toBe('pi-session:session');
      expect(getPiAwarenessAgentId(ctx)).toBe('pi:session');
      process.env.OCTOCODE_AGENT_ID = 'agent-env';
      expect(getPiAwarenessAgentId()).toBe('agent-env');
      delete process.env.OCTOCODE_AGENT_ID;
      expect(getPiAwarenessSessionId()).toContain(`pi-session:${process.pid}-`);
      expect(getPiAwarenessAgentId()).toContain(`pi:${process.pid}-`);
    } finally {
      if (previous === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previous;
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
