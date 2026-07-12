import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { auditUnverified, markVerified } from '../src/verify.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function makePending(db: DatabaseSync, workspacePath: string, testPlan: string): string {
  const claim = preFlightIntent(db, { agentId: 'agent-a', workspacePath, targetFiles: ['/tmp/agent-a-target.txt'], testPlan });
  if (!claim.ok) throw new Error('claim failed');
  releaseFileLock(db, { agentId: 'agent-a', runId: claim.run.run_id, status: 'PENDING' });
  return claim.run.run_id;
}

function tempDirWithLink(): { real: string; link: string; base: string } {
  const base = mkdtempSync(join(tmpdir(), 'oc-verify-scope-'));
  const real = join(base, 'real');
  const link = join(base, 'link');
  mkdirSync(real, { recursive: true });
  symlinkSync(real, link);
  return { real, link, base };
}

describe('workspace-scope symlink stability', () => {
  it('audits a symlink-scoped release through the real path', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      const runId = makePending(db, link, 'verify-symlink-fix');
      const result = auditUnverified(db, { workspacePath: real });
      expect(result.count).toBe(1);
      expect(result.unverified[0]?.run_id).toBe(runId);
    } finally { rmSync(base, { recursive: true, force: true }); }
  });

  it('marks a symlink-scoped release through the real path', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      makePending(db, link, 'verify-symlink-mark');
      const result = markVerified(db, { agentId: 'agent-a', allPending: true, workspacePath: real, message: 'symlink-scoped check passed' });
      expect(result.ok).toBe(true);
      if (result.ok) expect(result.count).toBe(1);
      expect(auditUnverified(db, { workspacePath: real }).count).toBe(0);
    } finally { rmSync(base, { recursive: true, force: true }); }
  });
});
