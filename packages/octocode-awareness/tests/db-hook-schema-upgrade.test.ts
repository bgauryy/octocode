import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { consolidateDatabase } from '../src/db-consolidation.js';
import { initDb } from '../src/db-init.js';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { hookReceipts, upsertHookReceipt } from '../src/hook-receipts.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';
import { openAwarenessStore } from '../src/coordination/open.js';

const dirs: string[] = [];
afterEach(() => { for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true }); });
const digest = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');

function paths() {
  const dir = mkdtempSync(join(tmpdir(), 'canonical-consolidation-'));
  dirs.push(dir);
  return { dir, source: join(dir, 'source.sqlite3'), destination: join(dir, 'destination.sqlite3') };
}

describe('canonical schema consolidation', () => {
  it.each(['claude', 'codex', 'copilot', 'cursor', 'gemini', 'opencode'] as const)('persists %s host observations', (host) => {
    const db = new DatabaseSync(':memory:');
    try {
      initDb(db);
      upsertHookReceipt(db, { workspacePath: '/workspace', host, event: 'stop', status: 'success' });
      expect(hookReceipts(db, '/workspace', host)).toHaveLength(1);
    } finally { db.close(); }
  });

  it('upgrades signals.expires_at from nullable TEXT to NOT NULL, backfilling any NULL rows', () => {
    const db = new DatabaseSync(':memory:');
    try {
      // Start with a canonical database, then downgrade signals to the intermediate v5 state
      initDb(db);
      db.exec('PRAGMA foreign_keys = OFF');
      // Recreate signals with nullable expires_at and restore all its indexes
      db.exec(`
        DROP TABLE signals;
        CREATE TABLE signals (
          signal_id TEXT PRIMARY KEY, workspace_path TEXT NOT NULL, artifact TEXT,
          repo TEXT, ref TEXT, from_agent TEXT NOT NULL, to_agent TEXT,
          kind TEXT NOT NULL, subject TEXT NOT NULL, body TEXT,
          files_json TEXT NOT NULL DEFAULT '[]', refs_json TEXT NOT NULL DEFAULT '[]',
          thread_id TEXT NOT NULL, reply_to TEXT, importance INTEGER NOT NULL DEFAULT 5,
          status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','resolved')),
          resolved_at TEXT, created_at TEXT NOT NULL, expires_at TEXT
        );
        INSERT INTO signals
          (signal_id, workspace_path, from_agent, kind, subject, files_json, refs_json, thread_id, created_at)
          VALUES ('sig-null-expiry', '/ws', 'agent-1', 'fyi', 'test', '[]', '[]', 'thread-1', '2026-01-01T00:00:00.000Z');
        CREATE INDEX idx_signals_status         ON signals(status);
        CREATE INDEX idx_signals_to_agent       ON signals(to_agent);
        CREATE INDEX idx_signals_workspace_path ON signals(workspace_path);
        CREATE INDEX idx_signals_scope          ON signals(workspace_path, artifact);
        CREATE INDEX idx_signals_created_at     ON signals(created_at);
        CREATE INDEX idx_signals_expires_at     ON signals(expires_at);
        CREATE INDEX idx_signals_thread         ON signals(thread_id);
      `);
      db.exec('PRAGMA foreign_keys = ON');
      // initDb must detect the nullable expires_at and upgrade it
      initDb(db);
      const cols = db.prepare('PRAGMA table_info(signals)').all() as Array<{ name: string; notnull: number }>;
      expect(cols.find(c => c.name === 'expires_at')?.notnull).toBe(1);
      const { expires_at } = db.prepare("SELECT expires_at FROM signals WHERE signal_id = 'sig-null-expiry'")
        .get() as { expires_at: string };
      expect(expires_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    } finally { db.close(); }
  });

  it('rejects the former host-enum schema without publishing or changing the source', () => {
    const { source, destination } = paths();
    const db = new DatabaseSync(source);
    db.exec(SCHEMA_DDL.replace("'claude','codex','copilot','cursor','gemini','opencode'", "'claude','codex','cursor'"));
    db.exec(SCHEMA_INDEX_DDL);
    db.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
    db.close();
    const before = digest(source);
    expect(() => consolidateDatabase(source, destination)).toThrow(/fingerprint mismatch/);
    expect(existsSync(destination)).toBe(false);
    expect(digest(source)).toBe(before);
  });

  it('copies an exact current canonical store to a new file', () => {
    const { dir, source, destination } = paths();
    const db = new DatabaseSync(source);
    initDb(db);
    upsertHookReceipt(db, { workspacePath: dir, host: 'codex', event: 'stop', status: 'success' });
    db.close();
    const before = digest(source);
    expect(consolidateDatabase(source, destination).copiedTables.hook_receipts).toBe(1);
    expect(digest(source)).toBe(before);
    const copied = new DatabaseSync(destination, { readOnly: true });
    try { expect(hookReceipts(copied, dir, 'codex')).toHaveLength(1); }
    finally { copied.close(); }
  });

  it('preserves a pruned canonical outbox high-water mark across the copy', () => {
    const { dir, source, destination } = paths();
    const db = new DatabaseSync(source);
    initDb(db);
    db.prepare("INSERT INTO event_consumers VALUES (?, 'reader', 40, '2026-09-06T00:00:00Z')").run(dir);
    db.prepare("INSERT INTO sqlite_sequence(name,seq) VALUES ('event_outbox',40)").run();
    db.close();
    consolidateDatabase(source, destination);
    const store = openAwarenessStore({ workspace: dir, dbPath: destination });
    try {
      const event = store.appendEvent(store.createHarnessEvent({ type: 'test', aggregateKind: 'audit', aggregateId: 'audit-1', payload: {} }));
      expect(event.sequence).toBe(41);
      expect(store.listEvents({ consumerId: 'reader' }).map(({ eventId }) => eventId)).toEqual([event.eventId]);
    } finally { store.close(); }
  });
});
