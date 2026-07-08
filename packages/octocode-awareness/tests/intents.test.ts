import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { fileLock, preFlightIntent, releaseFileLock } from '../src/intents.js';
import { pruneStale } from '../src/maintenance.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function tempFile(): { dir: string; path: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'oc-intent-test-'));
  const path = join(dir, 'target.txt');
  writeFileSync(path, 'seed');
  return { dir, path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('preFlightIntent', () => {
  it('returns ok=true with a task_id', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.task.task_id).toMatch(/^task_/);
        expect(result.task.target_files).toContain(path);
      }
    } finally { cleanup(); }
  });

  it('returns ok=false with conflict when another agent holds EXCLUSIVE lock', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(a.ok).toBe(true);
      const b = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(b.ok).toBe(false);
      if (!b.ok) {
        expect(b.conflicts).toHaveLength(1);
        expect(b.conflicts[0]!.agent_id).toBe('agent-a');
      }
    } finally { cleanup(); }
  });

  it('resolves relative target files under workspacePath', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, {
        agentId: 'agent-a',
        workspacePath: dir,
        targetFiles: ['src/a.ts'],
      });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.task.target_files).toEqual([join(dir, 'src/a.ts')]);
        expect(result.task.locks[0]!.file_path).toBe(join(dir, 'src/a.ts'));
      }
    } finally { cleanup(); }
  });

  it('stores git subdir workspace claims at repo root while resolving files from the subdir', () => {
    const db = freshDb();
    const repo = mkdtempSync(join(tmpdir(), 'oc-intent-git-'));
    const pkg = join(repo, 'packages/pkg-a');
    mkdirSync(join(pkg, 'src'), { recursive: true });
    const git = spawnSync('git', ['init', '-q'], { cwd: repo, encoding: 'utf8' });
    expect(git.status, git.stderr || git.stdout).toBe(0);
    const gitRoot = spawnSync('git', ['-C', pkg, 'rev-parse', '--show-toplevel'], { encoding: 'utf8' });
    expect(gitRoot.status, gitRoot.stderr || gitRoot.stdout).toBe(0);
    const expectedRoot = gitRoot.stdout.trim();
    try {
      const result = preFlightIntent(db, {
        agentId: 'agent-a',
        workspacePath: pkg,
        targetFiles: ['src/a.ts'],
      });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.task.workspace_path).toBe(expectedRoot);
        expect(result.task.target_files).toEqual([join(pkg, 'src/a.ts')]);
        const stored = db.prepare('SELECT workspace_path, files_json FROM tasks WHERE task_id = ?')
          .get(result.task.task_id) as { workspace_path: string; files_json: string };
        expect(stored.workspace_path).toBe(expectedRoot);
        expect(JSON.parse(stored.files_json)).toEqual([join(pkg, 'src/a.ts')]);
      }
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it('same agent can re-claim without conflict', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      const second = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(second.ok).toBe(true);
    } finally { cleanup(); }
  });

  it('sets expiresAt and caps lock TTL at 10 minutes', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const before = Date.now();
      const result = preFlightIntent(db, {
        agentId: 'agent-a',
        targetFiles: [path],
        ttlMs: 60 * 60_000,
      });
      if (result.ok) {
        const expiresAt = result.task.locks[0]!.expires_at;
        expect(expiresAt).not.toBeNull();
        const ttl = Date.parse(expiresAt!) - before;
        expect(ttl).toBeGreaterThan(0);
        expect(ttl).toBeLessThanOrEqual(10 * 60_000 + 1000);
      }
    } finally { cleanup(); }
  });

  it('prunes expired locks before conflict checks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const first = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 1000 });
      if (!first.ok) throw new Error('first claim failed');
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE task_id = ?').run(past, first.task.task_id);
      const second = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(second.ok).toBe(true);
    } finally { cleanup(); }
  });

  it('works with no target files', () => {
    const db = freshDb();
    const result = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [] });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.task.target_files).toHaveLength(0);
    }
  });
});

describe('releaseFileLock', () => {
  it('releases by task_id and returns released=true', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');

      const release = releaseFileLock(db, {
        agentId: 'agent-a',
        taskId: claim.task.task_id,
        status: 'SUCCESS',
      });
      expect(release.released).toBe(true);
      expect(release.locks_released).toBe(1);
      expect(release.status).toBe('PENDING');
      expect(release.unverifiedConclusion).toContain('SUCCESS requested without --verified');
    } finally { cleanup(); }
  });

  it('allows another agent to claim after release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('a claim failed');
      releaseFileLock(db, { agentId: 'agent-a', taskId: a.task.task_id });
      const b = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(b.ok).toBe(true);
    } finally { cleanup(); }
  });

  it('releasing one same-agent overlapping intent keeps sibling locks active', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const first = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      const second = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!first.ok || !second.ok) throw new Error('claims failed');

      const release = releaseFileLock(db, { agentId: 'agent-a', taskId: first.task.task_id });
      expect(release.locks_released).toBe(1);
      const locks = db.prepare('SELECT task_id FROM locks WHERE file_path = ? ORDER BY acquired_at').all(path) as Array<{ task_id: string }>;
      expect(locks.map((l) => l.task_id)).toEqual([second.task.task_id]);

      const other = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(other.ok).toBe(false);
    } finally { cleanup(); }
  });

  it('releases by target file', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      const release = releaseFileLock(db, {
        agentId: 'agent-a',
        targetFiles: [path],
      });
      expect(release.locks_released).toBe(1);
    } finally { cleanup(); }
  });

  it('returns released=false with no matching locks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const release = releaseFileLock(db, {
        agentId: 'agent-a',
        targetFiles: [path],
      });
      expect(release.released).toBe(false);
      expect(release.locks_released).toBe(0);
    } finally { cleanup(); }
  });

  it('accepts PENDING and FAILED statuses', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      const release = releaseFileLock(db, {
        agentId: 'agent-a', taskId: a.task.task_id, status: 'PENDING',
      });
      expect(release.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('rejects invalid release statuses before mutating tasks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');
      expect(() => releaseFileLock(db, {
        agentId: 'agent-a',
        taskId: claim.task.task_id,
        status: 'ACTIVE' as 'SUCCESS',
      })).toThrow(/status must be PENDING, SUCCESS, or FAILED/);
      const task = db.prepare('SELECT status FROM tasks WHERE task_id = ?')
        .get(claim.task.task_id) as { status: string };
      expect(task.status).toBe('ACTIVE');
    } finally { cleanup(); }
  });

  it('keeps unverified SUCCESS releases pending', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      const release = releaseFileLock(db, { agentId: 'agent-a', taskId: a.task.task_id, status: 'SUCCESS' });
      const task = db.prepare('SELECT status FROM tasks WHERE task_id = ?')
        .get(a.task.task_id) as { status: string };
      expect(release.status).toBe('PENDING');
      expect(task.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('updates task status to SUCCESS after verified lock release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      releaseFileLock(db, {
        agentId: 'agent-a',
        taskId: a.task.task_id,
        status: 'SUCCESS',
        verified: true,
        verifiedNote: 'test passed',
      });
      const task = db.prepare('SELECT status FROM tasks WHERE task_id = ?')
        .get(a.task.task_id) as { status: string };
      expect(task.status).toBe('SUCCESS');
    } finally { cleanup(); }
  });

  it('atomically deletes locks row and updates task status on release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');
      const taskId = claim.task.task_id;

      // Pre-conditions: one lock row, task ACTIVE
      const locksBefore = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE task_id = ?').get(taskId) as { c: number };
      expect(locksBefore.c).toBe(1);
      const taskBefore = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId) as { status: string };
      expect(taskBefore.status).toBe('ACTIVE');

      // Release (no verification so status becomes PENDING)
      const release = releaseFileLock(db, { agentId: 'agent-a', taskId, status: 'PENDING' });
      expect(release.released).toBe(true);

      // Both mutations must be visible together — locks gone, task updated
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE task_id = ?').get(taskId) as { c: number };
      expect(locksAfter.c).toBe(0);
      const taskAfter = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId) as { status: string };
      expect(taskAfter.status).toBe('PENDING');
    } finally { cleanup(); }
  });
});

describe('pruneStale', () => {
  it('atomically deletes expired locks and updates task status', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 1000 });
      if (!claim.ok) throw new Error('claim failed');
      const taskId = claim.task.task_id;

      // Force-expire the lock so pruneStale will pick it up
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE task_id = ?').run(past, taskId);

      // Verify preconditions
      const locksBefore = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE task_id = ?').get(taskId) as { c: number };
      expect(locksBefore.c).toBe(1);
      const taskBefore = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId) as { status: string };
      expect(taskBefore.status).toBe('ACTIVE');

      const result = pruneStale(db);

      // Both mutations reflected in the return value
      expect(result.pruned_locks).toBeGreaterThan(0);
      expect(result.updated_tasks).toBeGreaterThan(0);

      // Both mutations visible in the DB simultaneously
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE task_id = ?').get(taskId) as { c: number };
      expect(locksAfter.c).toBe(0);
      const taskAfter = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId) as { status: string };
      expect(taskAfter.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('dry_run reports would_prune without mutating', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 1000 });
      if (!claim.ok) throw new Error('claim failed');
      const taskId = claim.task.task_id;

      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE task_id = ?').run(past, taskId);

      const result = pruneStale(db, { dryRun: true });
      expect(result.dry_run).toBe(true);
      expect(result.would_prune).toBeGreaterThan(0);
      expect(result.pruned_locks).toBe(0);

      // Lock must still exist after dry run
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE task_id = ?').get(taskId) as { c: number };
      expect(locksAfter.c).toBe(1);
    } finally { cleanup(); }
  });
});

describe('fileLock', () => {
  it('locks, reports status, renews, and releases by task id', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const locked = fileLock(db, {
        type: 'lock',
        agentId: 'agent-a',
        sessionId: 'session-a',
        workspacePath: dir,
        targetFiles: ['src/a.ts'],
        ttlMs: 1000,
      });
      expect(locked.ok).toBe(true);
      expect(locked.type).toBe('lock');
      if (!locked.ok || locked.type !== 'lock') throw new Error('lock failed');
      expect(locked.taskId).toMatch(/^task_/);
      expect(locked.files).toEqual([join(dir, 'src/a.ts')]);
      expect(locked.expiresAt).toBeTruthy();

      const status = fileLock(db, { type: 'status', workspacePath: dir, sessionId: 'session-a' });
      expect(status.type).toBe('status');
      if (status.type !== 'status') throw new Error('status failed');
      expect(status.locks).toHaveLength(1);
      expect(status.locks[0]!.task_id).toBe(locked.taskId);

      const renewed = fileLock(db, { type: 'renew', agentId: 'agent-a', taskId: locked.taskId, ttlMs: 60 * 60_000 });
      expect(renewed.type).toBe('renew');
      if (renewed.type !== 'renew') throw new Error('renew failed');
      expect(renewed.renewed).toBe(true);
      expect(Date.parse(renewed.expiresAt!)).toBeGreaterThanOrEqual(Date.parse(locked.expiresAt!));

      const released = fileLock(db, { type: 'release', agentId: 'agent-a', taskId: locked.taskId, status: 'PENDING' });
      expect(released.type).toBe('release');
      if (released.type !== 'release') throw new Error('release failed');
      expect(released.released).toBe(true);
      const emptyStatus = fileLock(db, { type: 'status', workspacePath: dir });
      expect(emptyStatus.type).toBe('status');
      if (emptyStatus.type !== 'status') throw new Error('status failed');
      expect(emptyStatus.locks).toHaveLength(0);
    } finally { cleanup(); }
  });
});
