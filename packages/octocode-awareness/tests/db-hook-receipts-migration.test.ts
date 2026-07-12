import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { AWARENESS_APPLICATION_ID, connectDb } from '../src/db.js';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';

function priorDatabase(path: string, tamper = false): void {
  const db = new DatabaseSync(path);
  db.exec(SCHEMA_DDL);
  db.exec(SCHEMA_INDEX_DDL);
  db.exec('DROP TABLE hook_receipts');
  if (tamper) db.exec('ALTER TABLE memories ADD COLUMN tampered TEXT');
  db.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
  db.close();
}

describe('hook receipt schema migration', () => {
  it('transactionally migrates the exact prior persisted Awareness schema', () => {
    const dir = mkdtempSync(join(tmpdir(), 'octocode-receipt-migration-'));
    const path = join(dir, 'awareness.sqlite3');
    try {
      priorDatabase(path);
      const db = connectDb(path);
      expect(db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'hook_receipts'").get())
        .toEqual({ name: 'hook_receipts' });
      db.close();
      const reopened = connectDb(path);
      expect(reopened.prepare('PRAGMA integrity_check').get()).toEqual({ integrity_check: 'ok' });
      reopened.close();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('rejects a tampered prior-looking database without mutating it', () => {
    const dir = mkdtempSync(join(tmpdir(), 'octocode-receipt-tampered-'));
    const path = join(dir, 'awareness.sqlite3');
    try {
      priorDatabase(path, true);
      expect(() => connectDb(path)).toThrow(/canonical relation contract mismatch|canonical schema fingerprint mismatch/);
      const raw = new DatabaseSync(path);
      expect(raw.prepare("SELECT name FROM sqlite_schema WHERE name = 'hook_receipts'").get()).toBeUndefined();
      expect(raw.prepare('PRAGMA table_info(memories)').all()).toEqual(expect.arrayContaining([
        expect.objectContaining({ name: 'tampered' }),
      ]));
      raw.close();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
