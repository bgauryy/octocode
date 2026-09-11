import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openAwarenessStore } from '../src/coordination/open.js';
import { getAwarenessOperationDescriptor } from '../src/schema/operation-catalog.js';
import { MEMORY_LABELS } from '../src/schema/common.js';
import { memorySchemas } from '../src/schema/definitions-memory.js';

const roots: string[] = [];
const stores: Array<ReturnType<typeof openAwarenessStore>> = [];
afterEach(() => {
  stores.splice(0).forEach(store => store.close());
  roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true }));
});

function fixture(prefix: string) {
  const workspace = mkdtempSync(join(tmpdir(), prefix));
  roots.push(workspace);
  const awareness = openAwarenessStore({ workspace, dbPath: join(workspace, 'ledger.sqlite3') });
  stores.push(awareness);
  return { workspace, awareness };
}

describe('exact verified memory pointers', () => {
  it('recalls evidence only within its validity interval across retrieval modes', () => {
    const { awareness } = fixture('awareness-memory-validity-');
    const memoryId = awareness.storeVerifiedMemory({
      label: 'TEST', text: 'Future policy', sourceDigest: 'policy-v2',
      verifiedAt: '2026-10-01T00:00:00Z', validUntil: '2026-11-01T00:00:00Z',
    }).memoryId;
    for (const filter of [{ memoryId }, ...['lexical', 'semantic', 'hybrid'].map(mode => ({
      query: 'Future policy', mode: mode as 'lexical' | 'semantic' | 'hybrid',
    }))]) {
      for (const [now, count] of [
        ['2026-09-30T23:59:59Z', 0], ['2026-10-01T00:00:00Z', 1],
        ['2026-10-01T00:00:00.000Z', 1], ['2026-10-01T03:00:00+03:00', 1],
        ['2026-11-01T00:00:00Z', 0],
      ] as const) expect(awareness.recallVerifiedMemory({ ...filter, now }).memories).toHaveLength(count);
    }
  });

  it('advertises the actual label enum and rejects unsupported labels at the domain boundary', () => {
    const record = getAwarenessOperationDescriptor('memory.record')!.inputSchema as {
      properties: { label: { enum: string[] } };
    };
    expect(record.properties.label.enum).toEqual([...MEMORY_LABELS]);
    expect(memorySchemas.memory_recall.safeParse({ labels: ['unsupported'] }).success).toBe(false);
    const { awareness } = fixture('awareness-memory-label-');
    expect(() => awareness.storeVerifiedMemory({
      label: 'Awareness RFC history verification', text: 'Observed bytes', sourceDigest: 'sha256:source',
    })).toThrow(/label/i);
  });

  it('retrieves only the exact record and preserves source, scope and expiry filters', () => {
    const { awareness } = fixture('awareness-memory-pointer-');
    const common = {
      label: 'TEST', sourceDigest: 'sha256:source', scope: 'artifact' as const,
      artifact: 'fixture-artifact', verifiedAt: '2026-09-01T00:00:00Z', validUntil: '2026-10-01T00:00:00Z',
    };
    const id = awareness.storeVerifiedMemory({ ...common, text: 'Gamma verified source bytes and the original history.' }).memoryId;
    awareness.storeVerifiedMemory({ ...common, text: 'A different record with the same source.' });
    const params = {
      memoryId: id, sourceDigest: common.sourceDigest, scope: 'artifact' as const,
      artifact: common.artifact, now: '2026-09-09T00:00:00Z',
    };
    const result = awareness.recallVerifiedMemory(params);
    expect(result).toMatchObject({ memories: [{ memoryId: id, sourceDigest: common.sourceDigest }] });
    expect(result.memories).toHaveLength(1);
    for (const change of [
      { memoryId: 'missing' }, { sourceDigest: 'sha256:wrong' },
      { scope: 'project' as const }, { now: '2026-10-02T00:00:00Z' },
    ]) expect(awareness.recallVerifiedMemory({ ...params, ...change }).memories).toEqual([]);
    const other = mkdtempSync(join(tmpdir(), 'awareness-memory-other-'));
    roots.push(other);
    const foreign = openAwarenessStore({ workspace: other, dbPath: awareness.dbPath });
    stores.push(foreign);
    expect(foreign.recallVerifiedMemory(params).memories).toEqual([]);
  });

  it('rejects combining an exact pointer with a guessed search phrase', () => {
    const { awareness } = fixture('awareness-memory-pointer-');
    expect(() => awareness.recallVerifiedMemory({ memoryId: 'memory', query: 'guessed phrase' }))
      .toThrow('memory_id cannot be combined with query');
  });

  it('deduplicates exact replays, keeps timestamp variants, and distinguishes file evidence', () => {
    const { awareness } = fixture('awareness-memory-dedup-');
    const common = {
      label: 'TEST', text: 'Stable evidence decision', sourceDigest: 'sha256:stable',
      verifiedAt: '2026-09-01T00:00:00Z', validUntil: '2026-12-01T00:00:00Z',
    };
    const firstId = awareness.storeVerifiedMemory({ ...common, file: 'src/first.ts' }).memoryId;
    expect(awareness.storeVerifiedMemory({ ...common, file: 'src/first.ts' }).memoryId).toBe(firstId);
    expect(awareness.storeVerifiedMemory({ ...common, verifiedAt: '2026-09-02T00:00:00Z', file: 'src/first.ts' }).memoryId)
      .not.toBe(firstId);
    expect(awareness.storeVerifiedMemory({ ...common, file: 'src/second.ts' }).memoryId).not.toBe(firstId);
  });

  it('returns a deterministic continuation with a frozen validity instant', () => {
    const { awareness } = fixture('awareness-memory-pages-');
    for (const [index, source] of ['one', 'two', 'three'].entries()) awareness.storeVerifiedMemory({
      label: 'TEST', text: `Paged evidence ${source}`, sourceDigest: `sha256:paged-${source}`,
      verifiedAt: `2026-09-0${index + 1}T00:00:00Z`, validUntil: '2026-12-01T00:00:00Z',
    });
    const first = awareness.recallVerifiedMemory({ query: 'Paged evidence', limit: 1, now: '2026-09-30T00:00:00Z' });
    expect(first).toMatchObject({ memories: [expect.any(Object)], partial: true });
    expect(first.next?.call.params.now).toBe('2026-09-30T00:00:00Z');
    const continuation = first.next!.call.params;
    const next = awareness.recallVerifiedMemory({
      query: String(continuation.query), limit: Number(continuation.limit), offset: Number(continuation.offset),
      now: String(continuation.now), revision: String(continuation.revision),
    });
    expect(next.memories).toHaveLength(1);
    expect(next.memories[0]?.memoryId).not.toBe(first.memories[0]?.memoryId);
  });

  it('requires an existing history operation and makes supersession visible atomically', () => {
    const { awareness } = fixture('awareness-memory-history-');
    expect(() => awareness.storeVerifiedMemory({
      label: 'TEST', text: 'History backed decision', sourceDigest: 'sha256:history', historyRef: 'history:missing',
    })).toThrow('existing history operation');
    const oldId = awareness.storeVerifiedMemory({
      label: 'DECISION', text: 'Old decision', sourceDigest: 'sha256:old',
      verifiedAt: '2026-09-01T00:00:00Z', validUntil: '2026-12-01T00:00:00Z',
    }).memoryId;
    awareness.storeVerifiedMemory({
      label: 'DECISION', text: 'Replacement decision', sourceDigest: 'sha256:new',
      verifiedAt: '2026-09-02T00:00:00Z', validUntil: '2026-12-01T00:00:00Z', supersedes: [oldId],
    });
    expect(awareness.recallVerifiedMemory({ now: '2026-09-03T00:00:00Z' }).memories.map(item => item.sourceDigest))
      .toEqual(['sha256:new']);
  });
});
