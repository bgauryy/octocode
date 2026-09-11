import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import { openAwarenessStore } from '../src/coordination/open.js';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history-api.js';
import type { VerifiedMemoryPageV1, VerifiedMemoryRecallParams } from '../src/coordination/verified-memory.js';

const roots: string[] = [];
const stores: Array<ReturnType<typeof openAwarenessStore>> = [];
let restoreProcesses: () => void;
beforeEach(() => { restoreProcesses = allowLocalFixtureProcesses(); });
afterEach(() => {
  stores.splice(0).forEach(store => store.close());
  restoreProcesses();
  roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true }));
});

function fixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'verified-worktrees-')));
  roots.push(root);
  const main = join(root, 'main');
  const peer = join(root, 'peer with spaces');
  const clone = join(root, 'clone');
  const database = join(root, 'ledger.sqlite3');
  mkdirSync(main);
  const git = (cwd: string, ...args: string[]) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8' });
  git(main, 'init', '-q', '-b', 'main');
  git(main, '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'seed');
  git(main, 'worktree', 'add', '-qb', 'peer', peer);
  git(root, 'clone', '-q', main, clone);
  const open = (workspace: string) => {
    const store = openAwarenessStore({ workspace, dbPath: database });
    stores.push(store);
    return store;
  };
  const history = async (workspace: string, command: string, params: Record<string, unknown>) => {
    const db = connectDb(database);
    try { return await runAwarenessHistoryOperation(db, command, { workspace, ...params }); }
    finally { db.close(); }
  };
  return { main, peer, clone, database, open, history, git };
}

function resume(store: ReturnType<typeof openAwarenessStore>, call: { params: Record<string, unknown> }): VerifiedMemoryPageV1 {
  const params = call.params;
  return store.recallVerifiedMemory({
    ...(params.memory_id ? { memoryId: String(params.memory_id) } : {}),
    ...(params.query !== undefined ? { query: String(params.query) } : {}),
    ...(params.source_digest ? { sourceDigest: String(params.source_digest) } : {}),
    ...(params.scope ? { scope: params.scope as VerifiedMemoryRecallParams['scope'] } : {}),
    ...(params.artifact ? { artifact: String(params.artifact) } : {}),
    ...(params.file ? { file: params.file as string | string[] } : {}),
    ...(params.area ? { area: String(params.area) } : {}),
    ...(params.strict_scope !== undefined ? { strictScope: Boolean(params.strict_scope) } : {}),
    ...(params.limit !== undefined ? { limit: Number(params.limit) } : {}),
    ...(params.offset !== undefined ? { offset: Number(params.offset) } : {}),
    ...(params.now !== undefined ? { now: String(params.now) } : {}),
    ...(params.revision !== undefined ? { revision: String(params.revision) } : {}),
  });
}

describe('verified memories across Git worktrees', () => {
  it('exposes source history and distinguishes incomplete or unavailable evidence', async () => {
    const { main, peer, database, open, history } = fixture();
    writeFileSync(join(main, 'source.ts'), 'export const evidence = 1;\n');
    expect(await history(main, 'checkpoint', {
      operation_id: 'source-checkpoint', file: ['source.ts'], agent_id: 'author',
    })).toMatchObject({ ok: true });
    const source = open(main);
    const id = source.storeVerifiedMemory({
      label: 'TEST', text: 'A reusable source lesson', sourceDigest: 'source-v1',
      historyRef: 'source-checkpoint', file: 'source.ts',
    }).memoryId;
    const reader = open(peer);
    const recall = () => reader.recallVerifiedMemory({ memoryId: id });
    const page = recall();
    expect(page.memories[0]?.historyEvidence).toMatchObject({ state: 'recorded', next: { call: {
      command: 'history inspect', params: { operation_id: 'source-checkpoint', source_workspace: main },
    } } });
    const inspected = await history(peer, 'inspect', page.memories[0]!.historyEvidence!.next!.call.params);
    const inspection = inspected as { rows: Array<{ next: { after: { command: string; args: Record<string, unknown> } } }> };
    expect(inspection.rows).toHaveLength(1);
    const readCall = inspection.rows[0]!.next.after;
    expect(readCall.args).toMatchObject({ workspace: peer, source_workspace: main, operation_id: 'source-checkpoint', file: 'source.ts' });
    const read = await history(peer, readCall.command.replace(/^history /, ''), readCall.args);
    expect(Buffer.from(String(read.content), 'base64').toString()).toBe('export const evidence = 1;\n');
    expect(() => reader.storeVerifiedMemory({
      label: 'TEST', text: 'Foreign history cannot authorize a local store',
      sourceDigest: 'source-v1', historyRef: 'source-checkpoint',
    })).toThrow('existing history operation in this workspace');
    const db = new DatabaseSync(database);
    try {
      db.prepare("UPDATE local_history_operations SET status = 'partial' WHERE operation_id = ?").run('source-checkpoint');
      expect(recall().memories[0]?.historyEvidence?.state).toBe('incomplete');
      expect(reader.recallVerifiedMemory({ memoryId: id, revision: page.revision }))
        .toMatchObject({ partialReasons: ['snapshot_changed'], next: { call: { params: { memory_id: id, offset: 0 } } } });
      db.prepare('DELETE FROM local_history_versions WHERE operation_id = ?').run('source-checkpoint');
      db.prepare('DELETE FROM local_history_operations WHERE operation_id = ?').run('source-checkpoint');
      expect(recall().memories[0]?.historyEvidence).toMatchObject({ state: 'unavailable' });
      expect(recall().memories[0]?.historyEvidence?.next).toBeUndefined();
    } finally { db.close(); }
  });

  it('recalls exact sibling evidence with source-relative files and retains explicit filters', () => {
    const { main, peer, clone, open } = fixture();
    const memoryId = open(main).storeVerifiedMemory({
      label: 'TEST', text: 'Linked source evidence', sourceDigest: 'source-a', scope: 'artifact', artifact: 'parser',
      file: ['src/parser.ts', 'src/types.ts'], area: 'parsing',
      verifiedAt: '2026-09-01T00:00:00Z', validUntil: '2026-10-01T00:00:00Z',
    }).memoryId;
    const reader = open(peer);
    const params: VerifiedMemoryRecallParams = {
      memoryId, sourceDigest: 'source-a', scope: 'artifact', artifact: 'parser',
      file: ['src/parser.ts', join(peer, 'src/types.ts')], area: 'parsing', now: '2026-09-10T00:00:00Z',
    } as VerifiedMemoryRecallParams;
    expect(reader.recallVerifiedMemory(params)).toMatchObject({
      memories: [{ memoryId, workspacePath: main, file: ['src/parser.ts', 'src/types.ts'] }],
    });
    expect(reader.recallVerifiedMemory({ ...params, strictScope: true }).memories).toEqual([]);
    for (const changed of [
      { sourceDigest: 'other' }, { scope: 'project' as const }, { artifact: 'other' }, { file: 'parser.ts' },
      { file: ['src/parser.ts', 'src/missing.ts'] }, { area: 'other' }, { now: '2026-10-01T00:00:00Z' },
    ]) expect(reader.recallVerifiedMemory({ ...params, ...changed }).memories).toEqual([]);
    expect(open(clone).recallVerifiedMemory({ memoryId }).memories).toEqual([]);
  });

  it('executes complete sibling page chains and binds revisions to the query and physical caller', () => {
    const { main, peer, clone, open, git } = fixture();
    const ids = new Set<string>();
    for (const [index, workspace] of [main, peer, main].entries()) ids.add(open(workspace).storeVerifiedMemory({
      label: 'TEST', text: `Shared page ${index}`, sourceDigest: `source-${index}`, file: 'src/parser.ts',
    }).memoryId);
    const reader = open(peer);
    const first = reader.recallVerifiedMemory({ query: 'Shared page', file: 'src/parser.ts', limit: 1 });
    expect(first).toMatchObject({ partial: true, next: expect.any(Object) });
    const received = new Set(first.memories.map(memory => memory.memoryId));
    let page = first;
    while (page.next) {
      page = resume(reader, page.next.call);
      expect(page.partialReasons).not.toContain('snapshot_changed');
      for (const memory of page.memories) { expect(received.has(memory.memoryId)).toBe(false); received.add(memory.memoryId); }
    }
    expect(received).toEqual(ids);
    const changed = { ...first.next!.call, params: { ...first.next!.call.params, query: 'different' } };
    expect(resume(reader, changed).partialReasons).toEqual(['snapshot_changed']);
    for (const workspace of [main, clone]) expect(resume(open(workspace), first.next!.call).partialReasons)
      .toEqual(['snapshot_changed']);
    git(main, 'worktree', 'add', '-qb', 'new-peer', join(main, '..', 'new-peer'));
    expect(resume(reader, first.next!.call).partialReasons).toEqual(['snapshot_changed']);
  });
});
