import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { canonicalizePath } from '../src/git.js';
import { endWork, listWork, showWork, startWork, touchWork } from '../src/work.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function workspace(): { path: string; cleanup: () => void } {
  const path = mkdtempSync(join(tmpdir(), 'oc-work-test-'));
  return { path, cleanup: () => rmSync(path, { recursive: true, force: true }) };
}

const required = {
  rationale: 'refactor parser',
  testPlan: 'run parser tests',
};

describe('advisory work presence', () => {
  it('allows multiple agents to work on the same file and returns compact peers', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const first = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], ...required,
      });
      expect(first.ok).toBe(true);
      const second = startWork(db, {
        agentId: 'agent-b', workspacePath: ws.path, targetFiles: ['src/a.ts'],
        rationale: 'add tracing', testPlan: 'run tracing tests',
      });
      expect(second.ok).toBe(true);
      if (!second.ok) throw new Error('second presence failed');
      expect(second.peer_count).toBe(1);
      expect(second.peers[0]).toMatchObject({
        agent_id: 'agent-a', origin: 'WORK', file_path: canonicalizePath(join(ws.path, 'src/a.ts')), exclusive: false,
      });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get())
        .toEqual({ count: 2 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks').get()).toEqual({ count: 0 });
    } finally { ws.cleanup(); }
  });

  it('requires reasoning and a test plan for a new explicit WORK run', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      expect(() => startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], testPlan: 'test',
      })).toThrow(/rationale is required/);
      expect(() => startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], rationale: 'reason',
      })).toThrow(/test plan is required/);
    } finally { ws.cleanup(); }
  });

  it('creates a new explicit WORK unless the caller supplies a run id', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const first = startWork(db, {
        agentId: 'agent-a', sessionId: 'session-a', workspacePath: ws.path,
        targetFiles: ['src/a.ts'], ...required,
      });
      if (!first.ok) throw new Error('first start failed');
      const separate = startWork(db, {
        agentId: 'agent-a', sessionId: 'session-other', workspacePath: ws.path,
        targetFiles: ['src/a.ts'], ...required,
      });
      if (!separate.ok) throw new Error('second start failed');
      expect(separate.run.run_id).not.toBe(first.run.run_id);

      const reused = startWork(db, {
        agentId: 'agent-a', runId: first.run.run_id, workspacePath: ws.path,
        targetFiles: ['src/b.ts'],
      });
      if (!reused.ok) throw new Error('explicit reuse failed');
      expect(reused.run.run_id).toBe(first.run.run_id);
    } finally { ws.cleanup(); }
  });

  it('returns only affected files when reusing a run with large history', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const historical = Array.from({ length: 20 }, (_, index) => `src/file-${index}.ts`);
      const first = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: historical, ...required,
      });
      if (!first.ok) throw new Error('first start failed');
      expect(first.files).toHaveLength(20);

      const reused = startWork(db, {
        agentId: 'agent-a', runId: first.run.run_id, workspacePath: ws.path,
        targetFiles: ['src/file-0.ts'],
      });
      if (!reused.ok) throw new Error('reuse failed');
      expect(reused.run.run_id).toBe(first.run.run_id);
      expect(reused.files.map((file) => file.file_path))
        .toEqual([canonicalizePath(join(ws.path, 'src/file-0.ts'))]);
      expect(JSON.stringify(reused).length).toBeLessThan(1_500);
    } finally { ws.cleanup(); }
  });

  it('touches selected files and work end moves explicit runs to PENDING', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const started = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path,
        targetFiles: ['src/a.ts', 'src/b.ts'], ttlMs: 1_000, ...required,
      });
      if (!started.ok) throw new Error('start failed');
      const before = started.files.find((file) => file.file_path.endsWith('a.ts'))!.expires_at;
      const touched = touchWork(db, {
        agentId: 'agent-a', runId: started.run.run_id,
        targetFiles: ['src/a.ts'], ttlMs: 60_000,
      });
      expect(Date.parse(touched.files[0]!.expires_at)).toBeGreaterThan(Date.parse(before));

      const partial = endWork(db, {
        agentId: 'agent-a', runId: started.run.run_id, targetFiles: ['src/a.ts'],
      });
      expect(partial.run.status).toBe('ACTIVE');
      const ended = endWork(db, { agentId: 'agent-a', runId: started.run.run_id });
      expect(ended.run.status).toBe('PENDING');
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL')
        .get(started.run.run_id)).toEqual({ count: 0 });
    } finally { ws.cleanup(); }
  });

  it('lists and shows active work with derived run identity', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const started = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], ...required,
      });
      if (!started.ok) throw new Error('start failed');
      expect(listWork(db, { workspacePath: ws.path })).toMatchObject({ count: 1 });
      const shown = showWork(db, { workspacePath: ws.path, filePath: 'src/a.ts' });
      expect(shown.count).toBe(1);
      expect(shown.files[0]).toMatchObject({
        run_id: started.run.run_id, agent_id: 'agent-a', origin: 'WORK', rationale: required.rationale,
      });
    } finally { ws.cleanup(); }
  });

  it('bounds work rows while preserving exact totals', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      for (const file of ['src/a.ts', 'src/b.ts', 'src/c.ts']) {
        const started = startWork(db, {
          agentId: `agent-${file}`, workspacePath: ws.path, targetFiles: [file], ...required,
        });
        expect(started.ok).toBe(true);
      }
      const result = listWork(db, { workspacePath: ws.path, limit: 2 });
      expect(result).toMatchObject({ count: 2, total_count: 3, omitted_count: 1 });
      expect(result.files).toHaveLength(2);
    } finally { ws.cleanup(); }
  });
});

describe('exclusive work', () => {
  it('does not revive expired exclusivity after another run acquires the file', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const first = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], exclusive: true,
        rationale: 'first sensitive rewrite', testPlan: 'security suite',
      });
      if (!first.ok) throw new Error('first exclusive start failed');
      const past = '2000-01-01T00:00:00Z';
      db.prepare('UPDATE run_files SET expires_at = ? WHERE run_id = ?').run(past, first.run.run_id);
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(past, first.run.run_id);

      const second = startWork(db, {
        agentId: 'agent-b', workspacePath: ws.path, targetFiles: ['src/a.ts'], exclusive: true,
        rationale: 'replacement sensitive rewrite', testPlan: 'security suite',
      });
      if (!second.ok) throw new Error('second exclusive start failed');

      expect(() => touchWork(db, {
        agentId: 'agent-a', runId: first.run.run_id, targetFiles: ['src/a.ts'],
      })).toThrow(/conflict/i);
      expect(db.prepare(`SELECT COUNT(*) AS count FROM locks
        WHERE file_path = ? AND (expires_at IS NULL OR expires_at > datetime('now'))`)
        .get(canonicalizePath(join(ws.path, 'src/a.ts')))).toEqual({ count: 1 });
    } finally { ws.cleanup(); }
  });

  it('rejects exclusive escalation while another run is actively present', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], ...required,
      });
      const result = startWork(db, {
        agentId: 'agent-b', workspacePath: ws.path, targetFiles: ['src/a.ts'], exclusive: true,
        rationale: 'sensitive rewrite', testPlan: 'security suite',
      });
      expect(result.ok).toBe(false);
      if (result.ok) throw new Error('unexpected exclusive start');
      expect(result.conflicts[0]).toMatchObject({ agent_id: 'agent-a', conflict_type: 'ACTIVE_WORK' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks').get()).toEqual({ count: 0 });
    } finally { ws.cleanup(); }
  });

  it('blocks advisory work when another run holds an exclusive lock', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const exclusive = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['src/a.ts'], exclusive: true, ...required,
      });
      expect(exclusive.ok).toBe(true);
      const advisory = startWork(db, {
        agentId: 'agent-b', workspacePath: ws.path, targetFiles: ['src/a.ts'],
        rationale: 'parallel edit', testPlan: 'tests',
      });
      expect(advisory.ok).toBe(false);
      if (advisory.ok) throw new Error('unexpected advisory start');
      expect(advisory.conflicts[0]).toMatchObject({ agent_id: 'agent-a', conflict_type: 'EXCLUSIVE_LOCK' });
    } finally { ws.cleanup(); }
  });

  it('creates presence and its exclusive lock in the same transaction', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const result = startWork(db, {
        agentId: 'agent-a', workspacePath: ws.path, targetFiles: ['schema.sql'], exclusive: true, ...required,
      });
      if (!result.ok) throw new Error('exclusive start failed');
      expect(db.prepare(`SELECT rf.run_id, l.run_id AS lock_run
        FROM run_files rf JOIN locks l ON l.run_id = rf.run_id AND l.file_path = rf.file_path
        WHERE rf.run_id = ?`).get(result.run.run_id))
        .toEqual({ run_id: result.run.run_id, lock_run: result.run.run_id });
    } finally { ws.cleanup(); }
  });
});

describe('task and hook origins', () => {
  it('reuses an explicitly supplied TASK run without replacing task reasoning', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
        VALUES ('run_task', NULL, 'TASK', 'agent-a', 'task snapshot', 'task tests', 'ACTIVE', ?,
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`).run(ws.path);
      const result = startWork(db, {
        agentId: 'agent-a', runId: 'run_task', workspacePath: ws.path, targetFiles: ['src/a.ts'],
      });
      if (!result.ok) throw new Error('task presence failed');
      expect(result.run).toMatchObject({ run_id: 'run_task', origin: 'TASK', rationale: 'task snapshot' });
      expect(() => endWork(db, { agentId: 'agent-a', runId: 'run_task' }))
        .toThrow(/task submit or task release/);
    } finally { ws.cleanup(); }
  });

  it('rejects attaching a supplied run through a different workspace or artifact', () => {
    const db = freshDb();
    const firstWorkspace = workspace();
    const secondWorkspace = workspace();
    try {
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact,
         created_at, updated_at)
        VALUES ('run_scoped', NULL, 'TASK', 'agent-a', 'task snapshot', 'task tests', 'ACTIVE', ?, 'pkg-a',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`).run(firstWorkspace.path);

      expect(() => startWork(db, {
        agentId: 'agent-a', runId: 'run_scoped', workspacePath: secondWorkspace.path,
        artifact: 'pkg-a', targetFiles: ['src/a.ts'],
      })).toThrow(/workspace.*does not match/i);
      expect(() => startWork(db, {
        agentId: 'agent-a', runId: 'run_scoped', workspacePath: firstWorkspace.path,
        artifact: 'pkg-b', targetFiles: ['src/a.ts'],
      })).toThrow(/artifact.*does not match/i);
      expect(db.prepare("SELECT COUNT(*) AS count FROM run_files WHERE run_id = 'run_scoped'").get())
        .toEqual({ count: 0 });
    } finally {
      firstWorkspace.cleanup();
      secondWorkspace.cleanup();
    }
  });

  it('moves a completed HOOK fallback run to PENDING', () => {
    const db = freshDb();
    const ws = workspace();
    try {
      const started = startWork(db, {
        agentId: 'agent-a', origin: 'HOOK', source: 'HOOK', workspacePath: ws.path,
        targetFiles: ['src/a.ts'], rationale: 'hook edit', testPlan: 'hook verification',
      });
      if (!started.ok) throw new Error('hook start failed');
      expect(started.run.origin).toBe('HOOK');
      const ended = endWork(db, { agentId: 'agent-a', runId: started.run.run_id });
      expect(ended.run.status).toBe('PENDING');
    } finally { ws.cleanup(); }
  });
});
