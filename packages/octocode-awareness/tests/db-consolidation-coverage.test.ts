import { DatabaseSync } from '../src/sqlite.js';
import { describe, expect, it } from 'vitest';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import {
  assertNoHistoryRowsForConsolidation,
  assertValidSource,
  copyCommonTables,
  nullableText,
  scalar,
  text,
} from '../src/db-consolidation-validation.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';

describe('database consolidation validation boundaries', () => {
  it('preserves a canonical common-table row while allowing destination defaults', () => {
    const source = new DatabaseSync(':memory:');
    const destination = new DatabaseSync(':memory:');
    destination.exec(SCHEMA_DDL);
    destination.exec(SCHEMA_INDEX_DDL);
    source.exec(`CREATE TABLE signals (
      signal_id TEXT, workspace_path TEXT, artifact TEXT, repo TEXT, ref TEXT,
      from_agent TEXT, to_agent TEXT, kind TEXT, subject TEXT, files_json TEXT,
      refs_json TEXT, thread_id TEXT, reply_to TEXT, importance INTEGER,
      status TEXT, resolved_at TEXT, created_at TEXT
    )`);
    source.prepare('INSERT INTO signals VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)')
      .run('signal-1', '/repo', null, null, null, 'sender', null, 'fyi', 'subject', '[]', '[]', 'thread-1', null, 5, 'open', null, '2026-01-01T00:00:00Z');
    expect(copyCommonTables(source, destination)).toMatchObject({ signals: 1 });
    expect(destination.prepare('SELECT body, subject FROM signals WHERE signal_id = ?').get('signal-1')).toEqual({ body: null, subject: 'subject' });
    source.close();
    destination.close();
  });

  it('rejects foreign identities and unrecognized schema relations before copy', () => {
    const foreign = new DatabaseSync(':memory:');
    foreign.exec('PRAGMA application_id = 1234');
    expect(() => assertValidSource(foreign)).toThrow('unsupported source application_id 1234');
    foreign.close();

  });

  it('rejects common-table schema drift instead of dropping data', () => {
    const destination = new DatabaseSync(':memory:');
    destination.exec(SCHEMA_DDL);
    const extra = new DatabaseSync(':memory:');
    extra.exec(`CREATE TABLE signals (
      signal_id TEXT, workspace_path TEXT, artifact TEXT, repo TEXT, ref TEXT,
      from_agent TEXT, to_agent TEXT, kind TEXT, subject TEXT, body TEXT,
      files_json TEXT, refs_json TEXT, thread_id TEXT, reply_to TEXT,
      importance INTEGER, status TEXT, resolved_at TEXT, created_at TEXT, legacy_extra TEXT
    )`);
    expect(() => copyCommonTables(extra, destination)).toThrow('signals has unmappable columns legacy_extra');
    extra.close();
    destination.close();
  });

  it('keeps source-value narrowing explicit', () => {
    expect(scalar(null, 'test', 'field')).toBeNull();
    expect(scalar(3, 'test', 'field')).toBe(3);
    expect(() => scalar({ value: 1 }, 'test', 'field')).toThrow('unsupported SQLite value');
    expect(text('value', 'test', 'field')).toBe('value');
    expect(() => text('', 'test', 'field')).toThrow('is required');
    expect(nullableText(null, 'test', 'field')).toBeNull();
    expect(() => nullableText(3, 'test', 'field')).toThrow('must be text');
  });

  it('recognizes the Awareness application identity as a valid source identity', () => {
    const source = new DatabaseSync(':memory:');
    source.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
    expect(() => assertValidSource(source)).not.toThrow();
    source.close();
  });

  it('rejects SQLite-only consolidation when path-bound history exists', () => {
    const source = new DatabaseSync(':memory:');
    source.exec(SCHEMA_DDL);
    expect(() => assertNoHistoryRowsForConsolidation(source)).not.toThrow();
    source.prepare(`INSERT INTO local_history_operations
      (operation_id, workspace_path, agent_id, kind, status, request_hash, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)`).run('op_1', '/repo', 'agent', 'checkpoint', 'complete', 'hash', 'now', 'now');
    expect(() => assertNoHistoryRowsForConsolidation(source)).toThrow(/history-aware consolidation required/);
    source.close();
  });
});
