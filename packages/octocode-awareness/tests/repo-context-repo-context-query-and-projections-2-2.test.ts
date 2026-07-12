import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
import { attendAwareness } from '../src/attend.js';
import { queryAwareness } from '../src/repo-context.js';
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
function seedActiveFilePeers(db: DatabaseSync, workspace: string, file: string): void {
    const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    const future = new Date(Date.now() + 60000).toISOString();
    const past = new Date(Date.now() - 60000).toISOString();
    db.prepare(`INSERT INTO plans (plan_id, name, objective, lead_agent_id, status, workspace_path, artifact, doc_dir, created_at, updated_at)
     VALUES ('plan_file_work', 'Shared auth plan', 'Coordinate auth edits', 'agent-a', 'ACTIVE', ?, 'svc', '.octocode/plan/auth', ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO tasks (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
     VALUES ('task_file_work', 'plan_file_work', 'Edit auth', 'shared task reason', 'tests pass', 'IN_PROGRESS', 80, 'agent-a', ?, ?)`).run(now, now);
    for (const [index, agentId] of ['agent-a', 'agent-b', 'agent-c', 'agent-d', 'agent-expired'].entries()) {
        const runId = `run_peer_${index}`;
        db.prepare(`INSERT INTO task_runs (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, 'focused test', 'ACTIVE', ?, 'svc', ?, ?)`).run(runId, index === 0 ? 'task_file_work' : null, index === 0 ? 'TASK' : 'WORK', agentId, `reason ${index}`, workspace, now, now);
        db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`).run(runId, file, now, now, agentId === 'agent-expired' ? past : future);
    }
    db.prepare(`INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
     VALUES ('lock_peer', ?, 'run_peer_0', ?, ?)`).run(file, now, future);
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
it('routes attend.next through owned Claimed, then FilesUnderWork, then Inbox', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-next-'));
    try {
      const db = freshDb();
      const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      const future = new Date(Date.now() + 60_000).toISOString();
      const file = join(dir, 'src', 'auth.ts');

      db.prepare(`INSERT INTO plans
        (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
        VALUES ('plan_next', 'Next', 'Route mid-loop', 'owner', 'ACTIVE', ?, '.octocode/plan/next', ?, ?)`)
        .run(dir, now, now);
      db.prepare(`INSERT INTO tasks
        (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
        VALUES ('task_next', 'plan_next', 'Claimed work', 'reason', 'tests pass', 'IN_PROGRESS', 1, 'owner', ?, ?)`)
        .run(now, now);
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
        VALUES ('run_next', 'task_next', 'TASK', 'owner', 'reason', 'tests pass', 'ACTIVE', ?, ?, ?)`)
        .run(dir, now, now);
      db.prepare(`INSERT INTO task_claims
        (task_id, run_id, agent_id, claimed_at, heartbeat_at, expires_at)
        VALUES ('task_next', 'run_next', 'owner', ?, ?, ?)`)
        .run(now, now, future);

      const claimed = attendAwareness(db, {
        agentId: 'owner',
        workspacePath: dir,
        query: 'claimed mid-loop',
        compact: true,
      });
      expect(claimed.next).toBe("octocode-awareness task heartbeat --task-id 'task_next' --run-id 'run_next' --agent-id 'owner' --compact");

      const peer = attendAwareness(db, {
        agentId: 'peer',
        workspacePath: dir,
        query: 'peer sees claimed',
        compact: true,
      });
      expect(peer.next).not.toContain('task heartbeat');

      db.prepare(`INSERT INTO run_files
        (run_id, file_path, source, started_at, heartbeat_at, expires_at)
        VALUES ('run_next', ?, 'EXPLICIT', ?, ?, ?)`)
        .run(file, now, now, future);
      const files = attendAwareness(db, {
        agentId: 'peer',
        workspacePath: dir,
        query: 'files under work',
        compact: true,
      });
      expect(files.next).toContain('work show');
      expect(files.next).toContain('src/auth.ts');
      expect(files.next).toContain(`--workspace '${dir}'`);

      db.prepare(`UPDATE run_files SET ended_at = ?, expires_at = ?`).run(now, now);
      agentSignal(db, {
        action: 'publish',
        agentId: 'owner',
        toAgents: ['peer'],
        workspacePath: dir,
        kind: 'request',
        subject: 'inbox next',
        body: 'please ack',
        importance: 6,
      });
      const inbox = attendAwareness(db, {
        agentId: 'peer',
        workspacePath: dir,
        query: 'inbox mid-loop',
        compact: true,
      });
      expect(inbox.next).toContain('signal list');
      expect(inbox.next).toContain("--agent-id 'peer'");
      expect(inbox.next).toContain(`--workspace '${dir}'`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('groups active file work by relative path, caps peers, and shows exclusive lock state', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-file-work-'));
    try {
      const db = freshDb();
      const file = join(dir, 'src', 'auth.ts');
      seedActiveFilePeers(db, dir, file);

      const workboard = queryAwareness(db, { workspacePath: dir, artifact: 'svc', view: 'workboard', limit: 10 });
      const row = workboard.rows.find(item => item.column === 'FilesUnderWork');
      expect(row).toMatchObject({
        item_type: 'file',
        path: 'src/auth.ts',
        peer_count: 4,
        omitted_peer_count: 1,
        locked: true,
        lock_agent_id: 'agent-a',
      });
      expect(row?.agents).toEqual(['agent-a', 'agent-b', 'agent-c']);
      expect(row?.task_ids).toEqual(['task_file_work']);
      expect(row?.plan_ids).toEqual(['plan_file_work']);
      expect(row?.plans).toEqual(['Shared auth plan']);
      expect(row?.reasons).toEqual(['shared task reason', 'reason 1', 'reason 2']);
      expect(String(row?.path)).not.toContain(dir);

      const compact = attendAwareness(db, { workspacePath: dir, artifact: 'svc', query: 'auth', compact: true });
      expect(compact.workboard.FilesUnderWork?.[0]).toMatchObject({ path: 'src/auth.ts', peer_count: 4 });
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('agents');
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('reasons');
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('lock_expires_at');
      expect(Buffer.byteLength(JSON.stringify(compact), 'utf8')).toBeLessThan(2 * 1024);

      const profile = queryAwareness(db, { workspacePath: dir, artifact: 'svc', view: 'repo-profile' });
      expect(profile.rows).toContainEqual({ metric: 'active_locks', count: 1 });
      db.prepare("UPDATE locks SET expires_at = '2000-01-01T00:00:00Z'").run();
      const expiredProfile = queryAwareness(db, { workspacePath: dir, artifact: 'svc', view: 'repo-profile' });
      expect(expiredProfile.rows).toContainEqual({ metric: 'active_locks', count: 0 });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('routes bloat to one valid bounded review command when verify is clear', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-bloat-'));
    try {
      const { db } = seededDb(dir);
      mkdirSync(join(dir, '.octocode'), { recursive: true });
      writeFileSync(join(dir, '.octocode', 'KNOWLEDGE.md'), `${'x\n'.repeat(250)}`, 'utf8');
      // Clear mid-loop lanes so projection bloat drives next.
      db.prepare(`UPDATE task_runs SET status = 'SUCCESS'`).run();
      db.prepare(`UPDATE run_files SET ended_at = '2000-01-01T00:00:00Z', expires_at = '2000-01-01T00:00:00Z'`).run();
      db.prepare(`UPDATE signals SET status = 'resolved', resolved_at = '2000-01-01T00:00:00Z' WHERE status != 'resolved'`).run();
      db.prepare(`UPDATE refinements SET state = 'done' WHERE state != 'done'`).run();
      const result = attendAwareness(db, {
        workspacePath: dir,
        limit: 10,
        compact: true,
      });
      expect(result.next).toBe(
        `octocode-awareness query workboard --workspace '${dir}' --format json --limit 5 --compact`,
      );
      expect(result.next).not.toContain(';');
      expect(result.next).not.toContain('memory forget --workspace');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('routes role-dialogue queries to self-reflection-dialogue.md', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-dialogue-'));
    try {
      const { db } = seededDb(dir);
      const full = attendAwareness(db, {
        workspacePath: dir,
        query: 'role dialogue tutor student review',
        limit: 10,
        compact: false,
      });
      const leads = (full.drive_state?.resource_leads ?? []) as Array<Record<string, unknown>>;
      const sources = leads.map(lead => String(lead['source'] ?? ''));
      expect(sources.some(source => source.includes('self-reflection-dialogue.md'))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

});
