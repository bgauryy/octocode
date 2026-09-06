import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { connectDb, connectCachedDb, getDatabasePath, resolveDbPath } from '../src/db-runtime.js';
import { openAwarenessStore } from '../src/coordination/open.js';
import { AwarenessStore } from '../src/coordination/coordination-continuity.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

it('rolls back a caught nested host mutation without discarding outer work', () => {
  class TransactionProbe extends AwarenessStore {
    transaction(operation: () => void) { this.writeTransaction(operation); }
  }
  const store = new TransactionProbe({ dbPath: ':memory:' });
  try {
    store.transaction(() => {
      store.joinAgent({ agentId: 'outer' });
      expect(() => store.transaction(() => {
        store.joinAgent({ agentId: 'inner' });
        throw new Error('abort inner write');
      })).toThrow('abort inner write');
    });
    expect(store.listAgents({}).map(agent => agent.agentId)).toEqual(['outer']);
  } finally { store.close(); }
});

it('preserves in-memory identity across openers without sharing unrelated stores', () => {
  expect(resolveDbPath(':memory:')).toBe(':memory:');
  const first = connectCachedDb(':memory:');
  const second = connectCachedDb(':memory:');
  const host = openAwarenessStore({ dbPath: ':memory:' });
  try {
    expect(getDatabasePath(first)).toBe(':memory:');
    expect(getDatabasePath(second)).toBe(':memory:');
    expect(first).not.toBe(second);
    expect(host.dbPath).toBe(':memory:');
  } finally { first.close(); second.close(); host.close(); }
});

it('all openers create one identical entity set without duplicate domain tables', () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-single-store-'));
  roots.push(root);
  const domain = connectDb(join(root, 'domain.sqlite3'));
  const host = openAwarenessStore({ workspace: root, dbPath: join(root, 'host.sqlite3') });
  host.close();
  const reopened = connectDb(join(root, 'host.sqlite3'));
  try {
    const names = (db: typeof domain) => db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all().map(row => row.name);
    expect(names(domain)).toEqual(names(reopened));
    expect(names(domain)).toEqual(expect.arrayContaining(['awareness_plans', 'awareness_tasks', 'task_runs', 'run_files', 'awareness_locks', 'awareness_memories', 'awareness_agents', 'signals', 'signal_reads', 'event_outbox']));
    for (const duplicate of ['plans', 'tasks', 'work_presence', 'locks', 'memories', 'agents', 'messages', 'message_receipts']) expect(names(domain)).not.toContain(duplicate);
    expect(domain.prepare('PRAGMA foreign_key_check').all()).toEqual([]);
  } finally { domain.close(); reopened.close(); }
});
