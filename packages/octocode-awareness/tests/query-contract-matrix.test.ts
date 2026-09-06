import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import {
  AWARENESS_QUERY_VIEWS,
  type AwarenessQueryFormat,
} from '../src/repo-model.js';
import { formatAwarenessQueryResult, queryAwareness } from '../src/repo-query.js';

const FORMATS: AwarenessQueryFormat[] = ['json', 'table', 'csv', 'markdown', 'html'];

describe('Awareness query contract matrix', () => {
  it('executes every view through every formatter', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);

    for (const view of AWARENESS_QUERY_VIEWS) {
      const result = queryAwareness(db, { view, workspacePath: process.cwd(), limit: 2 });
      expect(result).toMatchObject({ ok: true, view });
      for (const format of FORMATS) {
        const rendered = formatAwarenessQueryResult(result, format);
        expect(rendered.length, `${view}/${format}`).toBeGreaterThan(0);
      }
    }
    db.close();
  });
});
