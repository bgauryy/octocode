import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { WORKER_LIFECYCLE_DDL } from '../src/db-worker-schema.js';

describe('complete entity fingerprint', () => {
  it.each([
    ['unknown index', 'CREATE INDEX unexpected_actor_index ON sessions(agent_id)'],
    ['malformed worker projection', 'CREATE TABLE worker_lifecycle_events(sequence INTEGER PRIMARY KEY, payload TEXT)'],
    ['ordinary table masquerading as FTS', 'DROP TABLE memories_fts; CREATE TABLE memories_fts(memory_id TEXT, task_context TEXT, observation TEXT, tags TEXT)'],
  ])('rejects %s without repairing the database', (_label, sql) => {
    const db = new DatabaseSync(':memory:');
    try {
      initDb(db);
      db.exec(sql!);
      const before = db.prepare('SELECT type, name, sql FROM sqlite_schema ORDER BY type, name').all();
      expect(() => initDb(db)).toThrow(/fingerprint mismatch/);
      expect(db.prepare('SELECT type, name, sql FROM sqlite_schema ORDER BY type, name').all()).toEqual(before);
    } finally { db.close(); }
  });

  it('leaves the exact optional worker projection untouched for explicit migration', () => {
    const db = new DatabaseSync(':memory:');
    try {
      initDb(db);
      db.exec(WORKER_LIFECYCLE_DDL);
      expect(() => initDb(db)).toThrow(/copy-on-write.*source has not been changed/i);
      expect(db.prepare("SELECT 1 AS present FROM sqlite_schema WHERE name='worker_lifecycle_events'").get())
        .toEqual({ present: 1 });
    } finally { db.close(); }
  });
});
