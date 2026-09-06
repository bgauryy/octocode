import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { AGENT_APPLICATION_ID, initOctocodeSchema } from '@octocodeai/octocode-shared/schema';
import { openAwarenessStore } from '../src/coordination/open.js';
import { connectDb } from '../src/db-runtime.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function relationNames(db: DatabaseSync): string[] {
  return (db.prepare(`SELECT name FROM sqlite_schema
    WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
    ORDER BY name`).all() as Array<{ name: string }>).map(({ name }) => name);
}

describe('strict Awareness database separation', () => {
  it('creates an advanced Awareness store with the historical OCT1 identity only', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-advanced-'));
    roots.push(root);
    const dbPath = join(root, 'awareness.sqlite3');

    const db = connectDb(dbPath);
    expect(db.prepare('PRAGMA application_id').get())
      .toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(relationNames(db)).toContain('awareness_plans');
    expect(relationNames(db)).not.toContain('agent_sessions');
    expect(relationNames(db)).not.toContain('octocode_meta');
    db.close();
  });

  it('creates coordination state in the same Awareness identity without Agent relations', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-coordination-'));
    roots.push(root);
    const dbPath = join(root, 'awareness.sqlite3');

    const coordination = openAwarenessStore({ workspace: join(root, 'workspace'), dbPath });
    coordination.createPlan({ title: 'Shared plan', goal: 'One lifecycle', agentId: 'owner' });
    coordination.close();

    const db = new DatabaseSync(dbPath);
    expect(db.prepare('PRAGMA application_id').get())
      .toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_plans').get()).toEqual({ count: 1 });
    expect(relationNames(db)).not.toContain('agent_sessions');
    expect(relationNames(db)).not.toContain('octocode_meta');
    db.close();
  });

  it('rejects incomplete schemas without silently rebuilding missing entities', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-legacy-'));
    roots.push(root);
    const dbPath = join(root, 'awareness.sqlite3');
    const legacy = connectDb(dbPath);
    legacy.exec('PRAGMA foreign_keys = OFF');
    legacy.exec(`
      DROP TABLE awareness_memories;
      DROP TABLE awareness_plans;
      DROP TABLE awareness_tasks;
      DROP TABLE awareness_locks;
      DROP TABLE awareness_agents;
    `);
    legacy.close();

    expect(() => connectDb(dbPath)).toThrow(/schema upgrade required/);
    const unchanged = new DatabaseSync(dbPath, { readOnly: true });
    expect(relationNames(unchanged)).not.toContain('awareness_plans');
    unchanged.close();
  });

  it.each([
    ['advanced opener', (dbPath: string, _workspace: string) => connectDb(dbPath).close()],
    ['coordination opener', (dbPath: string, workspace: string) => openAwarenessStore({ workspace, dbPath }).close()],
  ])('rejects an Agent database through the %s without changing it', (_label, open) => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-agent-store-'));
    roots.push(root);
    const dbPath = join(root, 'agent.sqlite3');
    const agent = new DatabaseSync(dbPath);
    initOctocodeSchema(agent);
    agent.exec(`PRAGMA application_id = ${AGENT_APPLICATION_ID}`);
    const before = relationNames(agent);
    expect(agent.prepare('PRAGMA application_id').get()).toEqual({ application_id: AGENT_APPLICATION_ID });
    agent.close();

    expect(() => open(dbPath, join(root, 'workspace'))).toThrow(/Agent SQLite store|application_id/);

    const inspect = new DatabaseSync(dbPath);
    expect(inspect.prepare('PRAGMA application_id').get()).toEqual({ application_id: AGENT_APPLICATION_ID });
    expect(relationNames(inspect)).toEqual(before);
    inspect.close();
  });

  it('refuses an unrelated database without changing its schema or identity', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-foreign-db-'));
    roots.push(root);
    const dbPath = join(root, 'foreign.sqlite3');
    const foreign = new DatabaseSync(dbPath);
    foreign.exec('CREATE TABLE cli_owned(value TEXT); PRAGMA application_id = 12345');
    foreign.close();

    expect(() => openAwarenessStore({ workspace: root, dbPath })).toThrow(/foreign Awareness application_id/);
    const inspect = new DatabaseSync(dbPath);
    expect(inspect.prepare('PRAGMA application_id').get()).toEqual({ application_id: 12345 });
    expect(relationNames(inspect)).toEqual(['cli_owned']);
    inspect.close();
  });
});
