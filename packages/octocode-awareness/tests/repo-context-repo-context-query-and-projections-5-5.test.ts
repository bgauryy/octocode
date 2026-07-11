import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { formatAwarenessQueryResult, injectRepoContext, queryAwareness } from '../src/repo-context.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

describe('repo context query and projections', () => {
it('rejects unknown views, formats, and repo injection modes', () => {
    const db = freshDb();
    expect(() => queryAwareness(db, { view: 'unknown' })).toThrow('unknown octocode-awareness query view');
    expect(() => formatAwarenessQueryResult(queryAwareness(db, { view: 'all' }), 'bad')).toThrow('--format must be');
    expect(() => injectRepoContext(db, { mode: 'publish' })).toThrow('--mode must be local or share');
  });

});
