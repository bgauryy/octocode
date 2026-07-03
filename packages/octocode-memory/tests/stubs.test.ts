import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { preFlightIntent } from '../src/intents.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock } from '../src/stubs.js';
import { auditUnverified } from '../src/verify.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function tempFile(): { dir: string; path: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'oc-stubs-test-'));
  const path = join(dir, 'f.txt');
  writeFileSync(path, 'seed');
  return { dir, path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('pruneStale', () => {
  it('returns 0 when no expired locks', () => {
    const db = freshDb();
    const result = pruneStale(db, {});
    expect(result.pruned_locks).toBe(0);
    expect(result.updated_intents).toBe(0);
  });

  it('prunes expired locks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 1000,
      });
      if (!result.ok) throw new Error('claim failed');
      // Age the lock to the past
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE file_locks SET expires_at = ? WHERE intent_id = ?')
        .run(past, result.intent.intent_id);

      const pruned = pruneStale(db, {});
      expect(pruned.pruned_locks).toBeGreaterThanOrEqual(1);
    } finally { cleanup(); }
  });

  it('sets intent to PENDING after lock expiry', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 1000,
      });
      if (!claim.ok) throw new Error('claim failed');
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE file_locks SET expires_at = ? WHERE intent_id = ?')
        .run(past, claim.intent.intent_id);

      pruneStale(db, {});
      const intent = db.prepare('SELECT status FROM agent_intents WHERE intent_id = ?')
        .get(claim.intent.intent_id) as { status: string };
      expect(intent.status).toBe('PENDING');
    } finally { cleanup(); }
  });
});

describe('auditUnverified', () => {
  it('returns ok=true and empty array when no PENDING intents exist', () => {
    const db = freshDb();
    const result = auditUnverified(db, {});
    expect(result.ok).toBe(true);
    expect(result.unverified).toHaveLength(0);
    expect(result.count).toBe(0);
  });
});

describe('notifyGet', () => {
  it('always returns ok=true and empty array', () => {
    const db = freshDb();
    const result = notifyGet(db, {});
    expect(result.ok).toBe(true);
    expect(result.count).toBe(0);
    expect(result.notifications).toHaveLength(0);
  });
});

describe('sessionCapture', () => {
  it('returns ok=true and captured=false', () => {
    const db = freshDb();
    const result = sessionCapture(db, {});
    expect(result.ok).toBe(true);
    expect(result.captured).toBe(false);
  });
});

describe('waitForLock', () => {
  it('returns ok=true and immediate', () => {
    const db = freshDb();
    const result = waitForLock(db, {});
    expect(result.ok).toBe(true);
    expect(result.waited_ms).toBe(0);
    expect(result.lock_free).toBe(true);
  });
});
