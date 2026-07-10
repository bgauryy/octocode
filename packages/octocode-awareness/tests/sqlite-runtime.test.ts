import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import {
  assertConcurrentWalSafe,
  assessConcurrentWalSafety,
  inspectSqliteRuntime,
  journalModeForSqliteVersion,
} from '../src/sqlite-runtime.js';

describe('concurrent WAL runtime gate', () => {
  it.each([
    ['3.44.6', true],
    ['3.44.7', true],
    ['3.50.7', true],
    ['3.51.3', true],
    ['3.51.4', true],
    ['3.52.0', true],
    ['4.0.0', true],
    ['3.44.5', false],
    ['3.45.3', false],
    ['3.50.6', false],
    ['3.51.2', false],
    ['3.7.0', false],
    ['not-a-version', false],
  ])('classifies SQLite %s safe=%s', (version, safe) => {
    expect(assessConcurrentWalSafety(version).safe).toBe(safe);
  });

  it('explains the supported fixed-version branches', () => {
    expect(assessConcurrentWalSafety('3.50.4')).toEqual({
      sqliteVersion: '3.50.4',
      safe: false,
      reason: expect.stringContaining('3.44.6, 3.50.7, or 3.51.3'),
    });
  });

  it('throws an actionable error for an unsafe runtime', () => {
    expect(() => assertConcurrentWalSafe('3.51.2')).toThrow(
      /SQLite 3\.51\.2.*unsafe for concurrent WAL.*upgrade/i,
    );
  });

  it('accepts a runtime containing the concurrent WAL fix', () => {
    expect(() => assertConcurrentWalSafe('3.51.3')).not.toThrow();
  });

  it('falls back to rollback journaling when concurrent WAL is unsafe', () => {
    expect(journalModeForSqliteVersion('3.50.4')).toBe('DELETE');
    expect(journalModeForSqliteVersion('3.50.7')).toBe('WAL');
  });

  it('reads the embedded SQLite version from the active connection', () => {
    const db = new DatabaseSync(':memory:');
    const expected = (db.prepare('SELECT sqlite_version() AS version').get() as { version: string }).version;

    expect(inspectSqliteRuntime(db)).toEqual(assessConcurrentWalSafety(expected));
  });
});
