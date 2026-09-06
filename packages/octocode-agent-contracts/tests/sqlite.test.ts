import { describe, expect, it } from 'vitest';
import {
  assertConcurrentWalSafe,
  assessConcurrentWalSafety,
  inspectSqliteRuntime,
  journalModeForSqliteVersion,
} from '../src/sqlite-version.js';
import {
  DatabaseSync,
  checkpointWal,
  isSqliteBusy,
  withSqliteBusyRetry,
} from '../src/sqlite.js';

describe('sqlite-version', () => {
  it('gates WAL on the reset-race fix', () => {
    expect(journalModeForSqliteVersion('3.50.4')).toBe('DELETE');
    expect(journalModeForSqliteVersion('3.50.7')).toBe('WAL');
    expect(journalModeForSqliteVersion('3.51.3')).toBe('WAL');
    expect(journalModeForSqliteVersion('4.0.0')).toBe('WAL');
    expect(journalModeForSqliteVersion('not-a-version')).toBe('DELETE');
  });

  it('reports safety reasons and throws when unsafe', () => {
    expect(assessConcurrentWalSafety('3.50.4')).toMatchObject({ safe: false });
    expect(() => assertConcurrentWalSafe('3.51.2')).toThrow(/unsafe for concurrent WAL/);
    expect(() => assertConcurrentWalSafe('3.51.3')).not.toThrow();
  });

  it('inspects a live connection version', () => {
    const db = new DatabaseSync(':memory:');
    try {
      const result = inspectSqliteRuntime(db);
      expect(typeof result.sqliteVersion).toBe('string');
      expect(typeof result.safe).toBe('boolean');
    } finally {
      db.close();
    }
  });
});

describe('sqlite runtime primitives', () => {
  it('classifies busy errors', () => {
    expect(isSqliteBusy(new Error('database is locked'))).toBe(true);
    expect(isSqliteBusy(Object.assign(new Error('x'), { errcode: 5 }))).toBe(true);
    expect(isSqliteBusy(new Error('syntax error'))).toBe(false);
    expect(isSqliteBusy('not an error')).toBe(false);
  });

  it('returns the operation result without retrying on success', () => {
    let calls = 0;
    const value = withSqliteBusyRetry(() => {
      calls += 1;
      return 42;
    });
    expect(value).toBe(42);
    expect(calls).toBe(1);
  });

  it('rethrows non-busy errors immediately', () => {
    expect(() => withSqliteBusyRetry(() => { throw new Error('nope'); })).toThrow('nope');
  });

  it('checkpointWal is non-fatal on :memory:', () => {
    const db = new DatabaseSync(':memory:');
    try {
      expect(() => checkpointWal(db)).not.toThrow();
    } finally {
      db.close();
    }
  });
});
