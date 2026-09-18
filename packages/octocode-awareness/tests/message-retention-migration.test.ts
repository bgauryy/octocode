import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyDatabaseMigration } from '../src/db-consolidation.js';
import { inspectSchemaState } from '../src/db-introspection.js';
import { initDb } from '../src/db-init.js';
import { signalExpiresAt } from '../src/message-lifecycle.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe('message retention schema migration', () => {
  it('backfills existing canonical signals deterministically', () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-message-retention-')));
    roots.push(root);
    const sourcePath = join(root, 'source.sqlite3');
    const destinationPath = join(root, 'destination.sqlite3');
    const db = new DatabaseSync(sourcePath);
    initDb(db);
    db.prepare('UPDATE awareness_meta SET schema_version = 4').run();
    db.exec('DROP INDEX IF EXISTS idx_signals_expires_at');
    db.exec('ALTER TABLE signals DROP COLUMN expires_at');
    const createdAt = '2026-01-02T03:04:05.000Z';
    db.prepare(`INSERT INTO signals
      (signal_id,workspace_path,from_agent,to_agent,kind,subject,body,files_json,refs_json,thread_id,reply_to,importance,status,resolved_at,created_at)
      VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).run(
      'existing-signal', '/repo', 'agent-a', 'agent-b', 'request', 'existing', null, '[]', '[]', 'existing-signal', null,
      5, 'open', null, createdAt,
    );
    db.close();

    const source = new DatabaseSync(sourcePath);
    expect(inspectSchemaState(source)).toBe('schema-generation-upgrade');
    source.close();
    const report = applyDatabaseMigration(sourcePath, destinationPath);
    expect(report.published).toBe(true);
    const destination = new DatabaseSync(destinationPath);
    const row = destination.prepare('SELECT expires_at FROM signals WHERE signal_id = ?').get('existing-signal') as { expires_at: string };
    expect(row.expires_at).toBe(signalExpiresAt('request', createdAt));
    expect(() => destination.prepare('UPDATE signals SET expires_at = NULL WHERE signal_id = ?').run('existing-signal'))
      .toThrow(/NOT NULL/i);
    expect(destination.prepare('PRAGMA table_info(signals)').all()).toEqual(expect.arrayContaining([
      expect.objectContaining({ name: 'expires_at' }),
    ]));
    destination.close();
  });
});
