import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
import { attendAwareness } from '../src/attend.js';
import { formatAwarenessQueryResult, queryAwareness, renderAwarenessHtml } from '../src/repo-context.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function seedPendingTasks(db: DatabaseSync, workspace: string, file: string): void {
    const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    for (const runId of ['run_pending_a', 'run_pending_b']) {
        db.prepare(`INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
       VALUES (?, 'WORK', ?, ?, ?, 'PENDING', ?, ?, ?, ?)`).run(runId, 'agent-a', 'verify auth file', 'vitest auth', workspace, 'svc', now, now);
        db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`).run(runId, file, now, now, new Date(Date.now() + 60000).toISOString());
    }
}
function seededDb(workspace: string): {
    db: DatabaseSync;
    file: string;
} {
    const db = freshDb();
    const file = join(workspace, 'src', 'auth.ts');
    registerAgent(db, {
        agentId: 'agent-a',
        agentName: 'Agent A',
        workspacePath: workspace,
        artifact: 'svc',
        context: 'repo-context test',
    });
    insertMemory(db, {
        agentId: 'agent-a',
        taskContext: 'auth gotcha',
        observation: 'Token migration order matters for auth',
        importance: 9,
        label: 'GOTCHA',
        tags: ['auth'],
        references: [`file:${file}`, 'https://example.com/auth-guide', 'repo:bgauryy/octocode-mcp', 'doc:auth-runbook'],
        workspacePath: workspace,
        artifact: 'svc',
        failureSignature: 'mechanism:auth|cause:order',
    });
    insertMemory(db, {
        agentId: 'agent-a',
        taskContext: 'auth decision',
        observation: 'Use schema before data backfill',
        importance: 8,
        label: 'DECISION',
        workspacePath: workspace,
        artifact: 'svc',
    });
    const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    const future = new Date(Date.now() + 60000).toISOString();
    db.prepare(`INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
     VALUES ('run_auth', 'WORK', 'agent-a', 'edit auth file', 'vitest auth', 'ACTIVE', ?, 'svc', ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
     VALUES ('run_auth', ?, 'EXPLICIT', ?, ?, ?)`).run(file, now, now, future);
    db.prepare(`INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
     VALUES ('lock_auth', ?, 'run_auth', ?, ?)`).run(file, now, future);
    insertRefinement(db, {
        agentId: 'agent-a',
        workspacePath: workspace,
        artifact: 'svc',
        reasoning: 'Continue auth cleanup',
        remember: 'Finish middleware after router',
        quality: 'handoff',
        state: 'open',
        files: [file],
    });
    agentSignal(db, {
        action: 'publish',
        agentId: 'agent-a',
        toAgents: ['agent-b'],
        workspacePath: workspace,
        artifact: 'svc',
        kind: 'decision',
        subject: 'auth order',
        body: 'schema first',
        files: [file],
        refs: ['doc:auth'],
        importance: 7,
    });
    insertEditLog(db, {
        agentId: 'agent-a',
        workspacePath: workspace,
        filePath: file,
        operation: 'update',
        linesAdded: 9,
        linesRemoved: 3,
    });
    seedPendingTasks(db, workspace, file);
    return { db, file };
}

describe('repo context query and projections', () => {
it('queries every view and renders all supported formats', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-context-'));
    try {
      const { db, file } = seededDb(dir);
      const base = { workspacePath: dir, artifact: 'svc', limit: 20 };

      for (const view of ['repo-profile', 'memories', 'gotchas', 'lessons', 'plans', 'tasks', 'runs', 'locks', 'agents', 'signals', 'refinements', 'files', 'activity', 'workboard'] as const) {
        const result = queryAwareness(db, { ...base, view, includeBodies: true });
        expect(result.ok).toBe(true);
        expect(result.view).toBe(view);
        expect(Array.isArray(result.rows)).toBe(true);
      }

      const workboard = queryAwareness(db, { ...base, view: 'workboard', limit: 10 });
      const verify = workboard.rows.filter(row => row.column === 'Verify');
      expect(verify).toHaveLength(2);
      expect(verify.flatMap(row => row.raw_ids as string[]))
        .toEqual(expect.arrayContaining(['run_pending_a', 'run_pending_b']));
      expect(workboard.rows.some(row => row.column === 'Inbox' && row.item_type === 'signal')).toBe(true);
      expect(workboard.rows.some(row => row.column === 'FilesUnderWork' && row.item_type === 'file')).toBe(true);

      const all = queryAwareness(db, { ...base, view: 'all', query: 'auth', file, includeBodies: true });
      expect(all.sections?.gotchas?.count).toBeGreaterThanOrEqual(1);
      expect(all.sections?.workboard?.count).toBeGreaterThanOrEqual(1);
      expect(all.sections?.files?.rows[0]?.file_path).toBe(file);
      expect(formatAwarenessQueryResult(all, 'json')).toContain('"view": "all"');
      expect(formatAwarenessQueryResult(all, 'csv')).toContain('section,count');
      expect(formatAwarenessQueryResult(all, 'table')).toContain('section');
      expect(formatAwarenessQueryResult(all, 'markdown')).toContain('# Awareness all');
      expect(formatAwarenessQueryResult(all, 'html')).toContain('<!doctype html>');
      expect(renderAwarenessHtml(all)).toContain('Octocode Awareness: all');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('builds a delta-sized compact attend packet with only actionable work', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-'));
    try {
      const { db, file } = seededDb(dir);
      const accessBefore = db.prepare(
        'SELECT COALESCE(SUM(access_count), 0) AS count FROM memories'
      ).get() as { count: number };
      const result = attendAwareness(db, {
        agentId: 'agent-a',
        workspacePath: dir,
        artifact: 'svc',
        query: 'auth',
        file,
        limit: 10,
        compact: true,
      });

      expect(result.ok).toBe(true);
      expect(result.counts).toMatchObject({ Ready: expect.any(Number), Claimed: expect.any(Number), Verify: 2, FilesUnderWork: expect.any(Number) });
      expect(result.workboard.Verify?.map(row => row.id)).toContain('run_pending_a');
      expect(result.evidence[0]?.why_selected.join(' ')).toContain('auth');
      expect(result.next).toContain('verify audit');
      expect(result.next).toContain("--run-id 'run_pending_");
      expect(result.next).not.toContain('--all-pending');
      expect(result).not.toHaveProperty('profile');
      expect(result).not.toHaveProperty('organ_state');
      expect(result).not.toHaveProperty('drive_state');
      expect(result).not.toHaveProperty('verification_targets');
      expect(JSON.stringify(result)).not.toContain('raw_ids');
      expect(Buffer.byteLength(JSON.stringify(result), 'utf8')).toBeLessThan(2 * 1024);
      const accessAfter = db.prepare(
        'SELECT COALESCE(SUM(access_count), 0) AS count FROM memories'
      ).get() as { count: number };
      expect(accessAfter.count).toBe(accessBefore.count);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('keeps global Verify totals exact while routing only the current agent', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-owner-'));
    try {
      const { db, file } = seededDb(dir);
      const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      const insertRun = db.prepare(`INSERT INTO task_runs
        (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
        VALUES (?, 'WORK', 'agent-a', 'bulk pending', 'bulk test', 'PENDING', ?, 'svc', ?, ?)`);
      const insertFile = db.prepare(`INSERT INTO run_files
        (run_id, file_path, source, started_at, heartbeat_at, expires_at, ended_at)
        VALUES (?, ?, 'EXPLICIT', ?, ?, ?, ?)`);
      db.exec('BEGIN');
      for (let index = 0; index < 501; index++) {
        const runId = `run_bulk_${String(index).padStart(4, '0')}`;
        insertRun.run(runId, dir, now, now);
        insertFile.run(runId, file, now, now, now, now);
      }
      db.exec('COMMIT');

      const peer = attendAwareness(db, {
        agentId: 'agent-b', workspacePath: dir, artifact: 'svc', compact: true,
      });
      expect(peer.counts?.Verify).toBe(503);
      expect(peer.next).not.toContain('verify audit');

      const owner = attendAwareness(db, {
        agentId: 'agent-a', workspacePath: dir, artifact: 'svc', compact: true,
      });
      expect(owner.counts?.Verify).toBe(503);
      expect(owner.next).toContain('verify audit');
      expect(owner.next).not.toContain('--all-pending');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('routes a submitted task to its exact pending run owner', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-task-owner-'));
    try {
      const db = freshDb();
      const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare(`INSERT INTO plans
        (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
        VALUES ('plan_verify', 'Verify', 'Route exact run', 'lead', 'ACTIVE', ?, '.octocode/plan/verify', ?, ?)`)
        .run(dir, now, now);
      db.prepare(`INSERT INTO tasks
        (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
        VALUES ('task_verify', 'plan_verify', 'Verify task', 'reason', 'tests pass', 'VERIFY', 1, 'lead', ?, ?)`)
        .run(now, now);
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
        VALUES ('run_verify_exact', 'task_verify', 'TASK', 'worker', 'reason', 'tests pass', 'PENDING', ?, ?, ?)`)
        .run(dir, now, now);

      const worker = attendAwareness(db, { agentId: 'worker', workspacePath: dir, compact: true });
      expect(worker.next).toContain("--run-id 'run_verify_exact'");
      expect(worker.next).toContain(`--workspace '${dir}'`);
      expect(worker.next).toContain("--agent-id 'worker'");
      const lead = attendAwareness(db, { agentId: 'lead', workspacePath: dir, compact: true });
      expect(lead.next).not.toContain('verify audit');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

});
