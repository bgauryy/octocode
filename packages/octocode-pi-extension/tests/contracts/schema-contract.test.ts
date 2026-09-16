import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AGENT_APPLICATION_ID, DatabaseSync } from '@octocodeai/octocode-awareness/host';
import { closeOctocodeDb, openOctocodeDb } from '../../src/contracts/db.js';

const directories: string[] = [];
afterEach(() => { directories.splice(0).forEach(dir => rmSync(dir, { recursive: true, force: true })); });

describe('Agent database canonical contract', () => {
  it.each([
    'DROP INDEX idx_agent_sessions_workspace',
    'DROP TABLE mcp_catalog_state',
    'ALTER TABLE agent_sessions ADD COLUMN obsolete TEXT',
    'CREATE TABLE foreign_state(value TEXT)',
    'CREATE TABLE memories_fts_hidden(value TEXT)',
    'CREATE VIEW memory_fts_hidden AS SELECT 1',
    'CREATE TRIGGER unexpected_trigger AFTER INSERT ON agent_sessions BEGIN DELETE FROM agent_sessions; END',
  ])('rejects schema drift before changing persisted state: %s', sql => {
    const directory = mkdtempSync(join(tmpdir(), 'agent-schema-'));
    directories.push(directory);
    const file = join(directory, 'agent.sqlite3');
    openOctocodeDb(file).exec(sql);
    closeOctocodeDb(file);
    const before = new DatabaseSync(file);
    const snapshot = before.prepare('SELECT type,name,sql FROM sqlite_schema ORDER BY type,name').all();
    before.close();
    expect(() => openOctocodeDb(file)).toThrow(/schema.*mismatch/);
    closeOctocodeDb(file);
    const after = new DatabaseSync(file, { readOnly: true });
    expect(after.prepare('SELECT type,name,sql FROM sqlite_schema ORDER BY type,name').all()).toEqual(snapshot);
    after.close();
  });

  it('does not initialize an empty database claiming an existing Agent identity', () => {
    const directory = mkdtempSync(join(tmpdir(), 'agent-schema-'));
    directories.push(directory);
    const file = join(directory, 'agent.sqlite3');
    const db = new DatabaseSync(file);
    db.exec(`PRAGMA application_id = ${AGENT_APPLICATION_ID}`);
    db.close();
    expect(() => openOctocodeDb(file)).toThrow(/schema.*mismatch/);
    closeOctocodeDb(file);
  });
});
