import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { preFlightIntent } from '../src/intents-preflight.js';
import { pruneStale } from '../src/maintenance-stale.js';
import { auditUnverified } from '../src/verify-audit.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function tempFile(): { path: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'oc-maintenance-test-'));
  const path = join(dir, 'f.txt');
  writeFileSync(path, 'seed');
  return { path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('pruneStale', () => {
  it('returns zero when no expired locks exist', () => {
    expect(pruneStale(freshDb(), {})).toEqual({ pruned_locks: 0 });
  });

  it('prunes an expired lock without changing its work state', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent', targetFiles: [path], ttlMs: 60_000 });
      if (!claim.ok) throw new Error('claim failed');
      const past = new Date(Date.now() - 5_000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE awareness_locks SET expires_at = ? WHERE run_id = ?').run(past, claim.run.run_id);

      expect(pruneStale(db, {}).pruned_locks).toBeGreaterThanOrEqual(1);
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'ACTIVE' });
    } finally {
      db.close();
      cleanup();
    }
  });
});

describe('auditUnverified', () => {
  it('returns an empty successful report when no pending work exists', () => {
    expect(auditUnverified(freshDb(), {})).toMatchObject({ ok: true, unverified: [], count: 0 });
  });
});
