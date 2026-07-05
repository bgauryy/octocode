import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';

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
  it('returns ok=true with an intent_id', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.intent.intent_id).toMatch(/^intent_/);
        expect(result.intent.target_files).toContain(path);
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
        const expiresAt = result.intent.locks[0]!.expires_at;
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
      db.prepare('UPDATE file_locks SET expires_at = ? WHERE intent_id = ?').run(past, first.intent.intent_id);
      const second = preFlightIntent(db, { agentId: 'agent-b', targetFiles: [path] });
      expect(second.ok).toBe(true);
    } finally { cleanup(); }
  });

  it('works with no target files', () => {
    const db = freshDb();
    const result = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [] });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.intent.target_files).toHaveLength(0);
    }
  });
});

describe('releaseFileLock', () => {
  it('releases by intent_id and returns released=true', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!claim.ok) throw new Error('claim failed');

      const release = releaseFileLock(db, {
        agentId: 'agent-a',
        intentId: claim.intent.intent_id,
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
      releaseFileLock(db, { agentId: 'agent-a', intentId: a.intent.intent_id });
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
        agentId: 'agent-a', intentId: a.intent.intent_id, status: 'PENDING',
      });
      expect(release.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('keeps unverified SUCCESS releases pending', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      const release = releaseFileLock(db, { agentId: 'agent-a', intentId: a.intent.intent_id, status: 'SUCCESS' });
      const intent = db.prepare('SELECT status FROM agent_intents WHERE intent_id = ?')
        .get(a.intent.intent_id) as { status: string };
      expect(release.status).toBe('PENDING');
      expect(intent.status).toBe('PENDING');
    } finally { cleanup(); }
  });

  it('updates intent status to SUCCESS after verified lock release', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const a = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      if (!a.ok) throw new Error('claim failed');
      releaseFileLock(db, {
        agentId: 'agent-a',
        intentId: a.intent.intent_id,
        status: 'SUCCESS',
        verified: true,
        verifiedNote: 'test passed',
      });
      const intent = db.prepare('SELECT status FROM agent_intents WHERE intent_id = ?')
        .get(a.intent.intent_id) as { status: string };
      expect(intent.status).toBe('SUCCESS');
    } finally { cleanup(); }
  });
});
