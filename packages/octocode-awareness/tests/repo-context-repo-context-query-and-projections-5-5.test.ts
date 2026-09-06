import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { formatAwarenessQueryResult, queryAwareness } from '../src/repo-query.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

describe('repo context query and projections', () => {
it('rejects unknown views, formats, and repo injection modes', () => {
    const db = freshDb();
    expect(() => queryAwareness(db, { view: 'unknown' })).toThrow('unknown npx @octocodeai/octocode-awareness query view');
    expect(() => formatAwarenessQueryResult(queryAwareness(db, { view: 'all' }), 'bad')).toThrow('--format must be');
  });

});
