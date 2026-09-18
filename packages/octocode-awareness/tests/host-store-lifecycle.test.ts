import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAwarenessClient } from '../src/client.js';
import { connectDb, openAwarenessStore } from '../src/host-api.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-host-store-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'awareness.sqlite3') };
}

describe('published host store lifecycle', () => {
  it('merges agent identity, projects presence, and preserves explicit leave state', () => {
    const { workspace, database } = fixture();
    const store = openAwarenessStore({ workspace, dbPath: database });
    try {
      expect(store.touchAgent({ agentId: 'new-agent' })).toMatchObject({ agentId: 'new-agent', status: 'ACTIVE' });
      expect(store.touchAgent({ agentId: 'left-agent', status: 'LEFT' })).toMatchObject({ status: 'LEFT' });
      expect(store.joinAgent({
        agentId: 'merged-agent', name: 'Stable Name', role: 'worker', metadata: '{"vendor":"first","keep":true}',
      })).toMatchObject({ agentId: 'merged-agent', name: 'Stable Name', role: 'worker' });
      const merged = store.joinAgent({ agentId: 'merged-agent', metadata: { vendor: 'second', added: true } });
      expect(merged).toMatchObject({ name: 'Stable Name', metadata: { vendor: 'second', keep: true, added: true } });
      expect(store.leaveAgent({ agentId: 'merged-agent' })).toMatchObject({ status: 'LEFT' });
      expect(store.listAgents().map(agent => agent.agentId)).toEqual(['new-agent']);
      expect(store.listAgents({ includeLeft: true }).map(agent => agent.agentId))
        .toEqual(expect.arrayContaining(['new-agent', 'left-agent', 'merged-agent']));

      const db = connectDb(database);
      try {
        db.prepare("UPDATE awareness_agents SET last_seen_at = '2000-01-01T00:00:00Z' WHERE agent_id = ?")
          .run('new-agent');
      } finally {
        db.close();
      }
      expect(store.listAgents({ staleAfterMs: 1 }).map(agent => agent.agentId)).toContain('new-agent');
    } finally {
      store.close();
    }
  });

  it('deduplicates, recalls, reindexes, forgets, and prunes host memory', () => {
    const { workspace, database } = fixture();
    const store = openAwarenessStore({ workspace, dbPath: database });
    try {
      expect(store.reindexMemories()).toEqual({ enabled: false, scanned: 0, embedded: 0 });
      const first = store.storeMemory({ label: 'BUILD', text: 'Use one canonical host store.', tags: ['host', 'canonical'] });
      const duplicate = store.storeMemory({ label: 'BUILD', text: 'Use one canonical host store.', tags: 'canonical,host' });
      expect(duplicate.memoryId).toBe(first.memoryId);
      expect(store.recallMemory({ query: 'canonical', label: ' BUILD ', limit: 999 }).memories)
        .toEqual([expect.objectContaining({ memoryId: first.memoryId })]);
      expect(store.recallMemory({ query: 'canonical', semantic: true, minSimilarity: 0.5 }).memories)
        .toEqual([expect.objectContaining({ memoryId: first.memoryId })]);
      expect(store.forgetMemory({ memoryId: 'missing' })).toEqual({ forgotten: false });
      expect(store.forgetMemory({ memoryId: first.memoryId })).toEqual({ forgotten: true });

      const oldBuild = store.storeMemory({ label: 'BUILD', text: 'Old build lesson.' });
      store.storeMemory({ label: 'TEST', text: 'Old test lesson.' });
      const db = connectDb(database);
      try {
        db.prepare("UPDATE awareness_memories SET created_at = '2000-01-01T00:00:00Z'").run();
      } finally {
        db.close();
      }
      expect(store.pruneMemories({ olderThanMs: 1, label: ' BUILD ', dryRun: true }))
        .toMatchObject({ dryRun: true, matched: 1, deleted: 0 });
      expect(store.pruneMemories({ olderThanMs: 1, label: 'BUILD', dryRun: false }))
        .toMatchObject({ dryRun: false, matched: 1, deleted: 1 });
      expect(store.forgetMemory({ memoryId: oldBuild.memoryId })).toEqual({ forgotten: false });
      expect(store.pruneMemories({ olderThanMs: 1_000_000_000_000, label: 'NOT_FOUND' }))
        .toMatchObject({ matched: 0, deleted: 0 });
    } finally {
      store.close();
    }
  });

  it('enforces message readership and prunes only old resolved messages', async () => {
    const { workspace, database } = fixture();
    const store = openAwarenessStore({ workspace, dbPath: database });
    try {
      const directed = store.sendMessage({
        fromAgentId: 'sender', toAgentId: 'reader', topic: 'Decision', text: 'Please review.',
        data: { type: 'memory.ready', payload: { memoryId: 'memory-1' } }, files: ['src/a.ts', 'src/b.ts'],
      });
      store.sendMessage({ fromAgentId: 'sender', text: 'Broadcast context.' });
      expect(store.countMessages({ agentId: 'reader' })).toBe(2);
      expect(store.listMessages({ agentId: 'reader', includeRead: false })).toHaveLength(2);
      expect(() => store.markMessageRead({ messageId: directed.messageId, agentId: 'sender' }))
        .toThrow('not addressed');
      expect(() => store.markMessageRead({ messageId: directed.messageId, agentId: 'stranger' }))
        .toThrow('not addressed');
      expect(() => store.markMessageRead({ messageId: 'missing', agentId: 'reader' })).toThrow('message not found');
      expect(store.markMessageRead({ messageId: directed.messageId, agentId: 'reader' })).toMatchObject({
        messageId: directed.messageId, readAt: expect.any(String),
      });

      const resolved = await createAwarenessClient({ workspace, database, agentId: 'reader' }).execute({
        operation: 'message.resolve', params: { signal_id: [directed.messageId] },
      });
      expect(resolved.exitCode, JSON.stringify(resolved.payload)).toBe(0);
      const db = connectDb(database);
      try {
        db.prepare("UPDATE signals SET created_at = '2000-01-01T00:00:00Z' WHERE signal_id = ?")
          .run(directed.messageId);
      } finally {
        db.close();
      }
      expect(store.pruneMessages({ olderThanMs: 1, readOnly: true, dryRun: true }))
        .toMatchObject({ dryRun: true, matched: 1, deleted: 0 });
      expect(store.pruneMessages({ olderThanMs: 1, readOnly: true, dryRun: false }))
        .toMatchObject({ dryRun: false, matched: 1, deleted: 1 });
      expect(store.pruneMessages({ olderThanMs: 1_000_000_000_000 }))
        .toMatchObject({ matched: 0, deleted: 0 });
    } finally {
      store.close();
    }
  });
});
