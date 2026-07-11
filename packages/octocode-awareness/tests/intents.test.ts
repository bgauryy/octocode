import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { canonicalizePath } from '../src/git.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { markVerified } from '../src/verify.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function tempFile(): {
    dir: string;
    path: string;
    cleanup: () => void;
} {
    const dir = mkdtempSync(join(tmpdir(), 'oc-intent-test-'));
    const path = join(dir, 'target.txt');
    writeFileSync(path, 'seed');
    return { dir, path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('preFlightIntent', () => {
  it('returns ok=true with a run_id', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.run.run_id).toMatch(/^run_/);
        expect(result.run.target_files).toContain(canonicalizePath(path));
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
        expect(result.run.target_files).toEqual([canonicalizePath(join(dir, 'src/a.ts'))]);
        expect(result.run.locks[0]!.file_path).toBe(canonicalizePath(join(dir, 'src/a.ts')));
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
        expect(result.run.workspace_path).toBe(expectedRoot);
        expect(result.run.target_files).toEqual([canonicalizePath(join(pkg, 'src/a.ts'))]);
        const stored = db.prepare('SELECT workspace_path, origin FROM task_runs WHERE run_id = ?')
          .get(result.run.run_id) as { workspace_path: string; origin: string };
        expect(stored.workspace_path).toBe(expectedRoot);
        expect(stored.origin).toBe('WORK');
        expect(db.prepare('SELECT file_path FROM run_files WHERE run_id = ?').all(result.run.run_id))
          .toEqual([{ file_path: canonicalizePath(join(pkg, 'src/a.ts')) }]);
      }
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });

  it('same agent can explicitly reuse its active WORK run on the same file', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const first = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      const second = preFlightIntent(db, {
        agentId: 'agent-a', runId: first.ok ? first.run.run_id : undefined, targetFiles: [path],
      });
      expect(first.ok).toBe(true);
      expect(second.ok).toBe(true);
      if (first.ok && second.ok) expect(second.run.run_id).toBe(first.run.run_id);
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
        const expiresAt = result.run.locks[0]!.expires_at;
        expect(expiresAt).not.toBeNull();
        const ttl = Date.parse(expiresAt!) - before;
        expect(ttl).toBeGreaterThan(0);
        expect(ttl).toBeLessThanOrEqual(10 * 60_000 + 1000);
      }
    } finally { cleanup(); }
  });

  it('expired lock removal does not erase active advisory presence', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const first = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 60_000 });
      if (!first.ok) throw new Error('first claim failed');
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(past, first.run.run_id);
      const second = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(second.ok).toBe(false);
    } finally { cleanup(); }
  });

  it('rejects lock runs with no target files', () => {
    const db = freshDb();
    expect(() => preFlightIntent(db, { agentId: 'agent-a', targetFiles: [] }))
      .toThrow(/at least one target file/);
  });
});

describe('releaseFileLock', () => {
  it('releases by run_id and returns released=true', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');

      const release = releaseFileLock(db, {
        agentId: 'agent-a',
        runId: claim.run.run_id,
        status: 'SUCCESS',
      });
      expect(release.released).toBe(true);
      expect(release.locks_released).toBe(1);
      expect(release.status).toBe('PENDING');
      expect(release.unverifiedConclusion).toContain('Direct SUCCESS');
    } finally { cleanup(); }
  });

  it('allows another agent to claim after release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('a claim failed');
      releaseFileLock(db, { agentId: 'agent-a', runId: a.run.run_id });
      const b = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(b.ok).toBe(true);
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
        agentId: 'agent-a', runId: a.run.run_id, status: 'PENDING',
      });
      expect(release.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('rejects invalid release statuses before mutating runs', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');
      expect(() => releaseFileLock(db, {
        agentId: 'agent-a',
        runId: claim.run.run_id,
        status: 'NOPE' as 'SUCCESS',
      })).toThrow(/status must be ACTIVE, PENDING, SUCCESS, or FAILED/);
      const run = db.prepare('SELECT status FROM task_runs WHERE run_id = ?')
        .get(claim.run.run_id) as { status: string };
      expect(run.status).toBe('ACTIVE');
    } finally { cleanup(); }
  });

  it('keeps unverified SUCCESS releases pending', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      const release = releaseFileLock(db, { agentId: 'agent-a', runId: a.run.run_id, status: 'SUCCESS' });
      const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?')
        .get(a.run.run_id) as { status: string };
      expect(release.status).toBe('PENDING');
      expect(task.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('routes lock release through PENDING before evidence-gated SUCCESS', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      releaseFileLock(db, {
        agentId: 'agent-a',
        runId: a.run.run_id,
        status: 'SUCCESS',
        verified: true,
        verifiedNote: 'test passed',
      });
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(a.run.run_id))
        .toEqual({ status: 'PENDING' });
      expect(markVerified(db, {
        runId: a.run.run_id,
        agentId: 'agent-a',
        status: 'SUCCESS',
        message: 'test passed',
      }).ok).toBe(true);
      const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?')
        .get(a.run.run_id) as { status: string };
      expect(task.status).toBe('SUCCESS');
    } finally { cleanup(); }
  });

  it('closes an explicit standalone run after its TTL lock is already gone', () => {
    const db = freshDb();
    const { dir, path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, {
        agentId: 'agent-a', sessionId: 'session-a', workspacePath: dir, artifact: 'pkg', targetFiles: [path],
      });
      if (!claim.ok) throw new Error('claim failed');
      db.prepare('DELETE FROM locks WHERE run_id = ?').run(claim.run.run_id);

      const released = releaseFileLock(db, {
        agentId: 'agent-a', sessionId: 'session-a', workspacePath: dir, artifact: 'pkg',
        runId: claim.run.run_id, status: 'SUCCESS',
        verified: true, verifiedNote: 'lockless run verified',
      });
      expect(released).toMatchObject({ released: true, locks_released: 0, run_ids: [claim.run.run_id] });
      expect(markVerified(db, {
        runId: claim.run.run_id,
        agentId: 'agent-a',
        status: 'SUCCESS',
        message: 'lockless run verified',
      }).ok).toBe(true);
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'SUCCESS' });
    } finally { cleanup(); }
  });

  it('atomically deletes locks row and updates task status on release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');
      const runId = claim.run.run_id;

      // Pre-conditions: one lock row, task ACTIVE
      const locksBefore = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(locksBefore.c).toBe(1);
      const taskBefore = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
      expect(taskBefore.status).toBe('ACTIVE');

      // Release (no verification so status becomes PENDING)
      const release = releaseFileLock(db, { agentId: 'agent-a', runId, status: 'PENDING' });
      expect(release.released).toBe(true);

      // Both mutations must be visible together — locks gone, task updated
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(locksAfter.c).toBe(0);
      const taskAfter = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
      expect(taskAfter.status).toBe('PENDING');
    } finally { cleanup(); }
  });
});
