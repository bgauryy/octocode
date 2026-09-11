import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';
import { DatabaseSync } from 'node:sqlite';

let workspace: string;
let store: AwarenessStore;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-continuation-'));
  store = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
});

afterEach(async () => {
  store.close();
  await rm(workspace, { recursive: true, force: true });
});

describe('canonical handoff continuation workflow', () => {
  it('saves, reads, and clears through one handoff owner', () => {
    const saved = store.addHandoff({
      agentId: 'agent-a',
      summary: 'Parser changes are ready for verification.',
      files: ['src/parser.ts', 'tests/parser.test.ts'],
    });

    expect(store.listHandoffs()).toMatchObject([{
      handoffId: saved.handoffId,
      agentId: 'agent-a',
      summary: 'Parser changes are ready for verification.',
      files: ['src/parser.ts', 'tests/parser.test.ts'],
      clearedAt: null,
    }]);
    expect(store.clearHandoff({ handoffId: saved.handoffId })).toEqual({ cleared: true });
    expect(store.listHandoffs()).toEqual([]);
    expect(store.listHandoffs({ includeCleared: true })).toMatchObject([{ handoffId: saved.handoffId }]);
    expect(store.clearHandoff({ handoffId: saved.handoffId })).toEqual({ cleared: false });
    const db = new DatabaseSync(join(workspace, 'awareness.sqlite3'), { readOnly: true });
    expect(db.prepare('SELECT COUNT(*) AS count FROM handoffs').get()).toEqual({ count: 0 });
    expect(db.prepare("SELECT COUNT(*) AS count FROM signals WHERE kind = 'handoff'").get()).toEqual({ count: 1 });
    expect(db.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type = 'peer.message'").get()).toEqual({ count: 1 });
    db.close();
  });

  it('keeps continuations workspace scoped without creating session or refinement ceremony', () => {
    const otherWorkspace = workspace + '-other';
    const other = openAwarenessStore({ workspace: otherWorkspace, dbPath: join(workspace, 'awareness.sqlite3') });
    try {
      store.addHandoff({ agentId: 'agent-a', summary: 'Resume the focused check.' });
      expect(other.listHandoffs()).toEqual([]);
    } finally {
      other.close();
    }
  });
});
