import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { executeKnowledgeMemory, getKnowledgeBriefing } from '../src/knowledge-memory.js';
import { normalizeAnchors } from '../src/knowledge-anchor.js';
import { knowledgeGetSchema } from '../src/knowledge-contract.js';

const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).reverse().forEach(fn => fn()));
function fixture() {
  const workspace = mkdtempSync(join(tmpdir(), 'knowledge-'));
  const db = new DatabaseSync(':memory:');
  initDb(db);
  cleanup.push(() => rmSync(workspace, { recursive: true, force: true }), () => db.close());
  const binding = { workspace, actorId: 'writer', sessionId: 'session-1' };
  const run = (operation: 'memory.set' | 'memory.get' | 'memory.revalidate', input: unknown) => executeKnowledgeMemory(db, binding, operation, input);
  return { db, workspace, binding, run };
}
function entry(key = 'cache') {
  return { key, title: 'Cache lesson', lesson: 'Invalidate on write', why: 'Readers otherwise see stale syntax',
    anchors: [{ kind: 'directory', value: 'src/cache' }], expected_revision: null, request_id: `create-${key}` };
}

describe('anchored knowledge revisions', () => {
  it('requires CAS, preserves historic rationale, and replays retries after supersession', async () => {
    const { run, db } = fixture();
    await expect(run('memory.set', { ...entry(), expected_revision: undefined })).rejects.toThrow();
    const first = await run('memory.set', entry());
    expect(first).toMatchObject({ status: 'created', memory: { why: entry().why, attribution: { actor_id: 'writer', session_id: 'session-1' } } });
    const revision = first.memory!.revision;
    expect(await run('memory.set', entry())).toMatchObject({ status: 'replayed', memory: { revision } });
    expect(await run('memory.set', { ...entry(), request_id: 'competing' })).toMatchObject({ status: 'conflict', current_revision: revision });
    const update = { ...entry(), lesson: 'Invalidate after durable write', why: 'Do not publish partial content', expected_revision: revision, request_id: 'update' };
    const second = await run('memory.set', update);
    expect(second).toMatchObject({ status: 'updated' });
    expect(await run('memory.set', { ...update, request_id: 'concurrent' })).toMatchObject({ status: 'conflict', current_revision: second.memory!.revision });
    expect(await run('memory.set', entry())).toMatchObject({ status: 'replayed', memory: { revision, state: 'superseded' } });
    expect(await run('memory.set', { ...entry(), why: 'altered retry' })).toMatchObject({ status: 'conflict', code: 'REQUEST_ID_REUSED' });
    expect(await run('memory.get', { key: 'cache', revision })).toMatchObject({ memories: [{ why: entry().why, state: 'superseded', superseded_by: second.memory!.revision }] });
    expect(await run('memory.get', { key: 'cache' })).toMatchObject({ memories: [{ why: update.why, state: 'current' }] });
    expect(db.prepare('SELECT COUNT(*) AS n FROM awareness_memories').get()).toEqual({ n: 2 });
    expect(db.prepare('SELECT verified_at FROM awareness_memories').all()).toEqual([{ verified_at: null }, { verified_at: null }]);
  });

  it('normalizes anchors safely and matches directory descendants at boundaries', async () => {
    const { run, workspace, db, binding } = fixture();
    expect(normalizeAnchors(workspace, [{ kind: 'file', value: join(workspace, 'src', '..', 'a.ts') }])).toEqual([{ kind: 'file', value: 'a.ts' }]);
    expect(() => normalizeAnchors(workspace, [{ kind: 'file', value: '../escape' }])).toThrow(/workspace/);
    expect(() => normalizeAnchors(workspace, [{ kind: 'file', value: 'src\\escape' }])).toThrow();
    expect(() => normalizeAnchors(workspace, [{ kind: 'flow', value: 'build\u001b[31m' }])).toThrow(/control/);
    expect(() => normalizeAnchors(workspace, [{ kind: 'file', value: 'a\0b.ts' }])).toThrow(/control/);
    expect(() => normalizeAnchors(workspace, [{ kind: 'flow', value: 'build\0publish' }])).toThrow(/control/);
    await run('memory.set', entry());
    expect(await run('memory.get', { anchors: [{ kind: 'file', value: 'src/cache/entry.ts' }] })).toMatchObject({ memories: [{ matched_by: [{ kind: 'directory', value: 'src/cache' }] }] });
    expect((await run('memory.get', { anchors: [{ kind: 'file', value: 'src/cache-old/entry.ts' }] })).memories).toEqual([]);
    expect((await getKnowledgeBriefing(db, binding, { files: ['src/cache/entry.ts'] })).memories).toHaveLength(1);
  });

  it('retrieves flow, failure and explicit symbol identity independently', async () => {
    const { run, db } = fixture();
    await run('memory.set', { ...entry(), anchors: [{ kind: 'flow', value: 'build -> publish' }, { kind: 'failure', value: 'CACHE_STALE' }, { kind: 'symbol', value: 'src/cache.ts#invalidate' }] });
    for (const anchor of [{ kind: 'flow', value: 'build -> publish' }, { kind: 'failure', value: 'CACHE_STALE' }, { kind: 'symbol', value: 'src/cache.ts#invalidate' }]) {
      expect((await run('memory.get', { anchors: [anchor] })).memories).toHaveLength(1);
    }
    expect((await run('memory.get', { failure_signature: 'CACHE_STALE' })).memories).toHaveLength(1);
    expect((await run('memory.get', { query: 'stale syntax' })).memories).toHaveLength(1);
    expect((await run('memory.get', { anchors: [{ kind: 'symbol', value: 'src/cache.ts#renamed' }] })).memories).toEqual([]);
    const refs = db.prepare('SELECT reference FROM memory_refs').all().map(row => row.reference);
    expect(refs.some(ref => String(ref).startsWith('file:'))).toBe(false);
    expect(refs).toContain('knowledge-anchor:flow:build -> publish');
  });

  it('rejects secrets in rationale, anchors and evidence before writing', async () => {
    const { run, db } = fixture();
    const secret = 'ghp_' + 'a'.repeat(36);
    for (const addition of [{ why: secret }, { evidence_refs: [secret] }, { anchors: [{ kind: 'flow', value: secret }] }]) {
      await expect(run('memory.set', { ...entry(), ...addition })).rejects.toThrow(/secret/);
    }
    expect(db.prepare('SELECT COUNT(*) AS n FROM awareness_memories').get()).toEqual({ n: 0 });
  });

  it('reports observed file freshness without verifying caller claims or mutating revisions', async () => {
    const { run, workspace } = fixture();
    writeFileSync(join(workspace, 'cache.ts'), 'v1');
    const input = { ...entry(), applicability: { files: ['cache.ts'], capture_fingerprint: true } };
    const saved = await run('memory.set', input);
    expect(await run('memory.get', { key: 'cache' })).toMatchObject({ memories: [{ evidence: { state: 'fresh', claim_verification: 'unverified' } }] });
    writeFileSync(join(workspace, 'cache.ts'), 'v2');
    expect(await run('memory.get', { key: 'cache' })).toMatchObject({ memories: [{ evidence: { state: 'stale' } }] });
    expect(await run('memory.set', input)).toMatchObject({ status: 'replayed', memory: { revision: saved.memory!.revision } });
    expect(await run('memory.revalidate', {})).toMatchObject({ review_queue: [{ revision: saved.memory!.revision, evidence: { state: 'stale' } }] });
    await run('memory.set', { ...entry('unknown') });
    expect(await run('memory.get', { key: 'unknown' })).toMatchObject({ memories: [{ evidence: { state: 'unknown', claim_verification: 'unverified' } }] });
  });

  it('executes byte-budget continuations to cover every matching row', async () => {
    const { run } = fixture();
    for (let index = 0; index < 7; index++) await run('memory.set', { ...entry(`cache-${index}`), lesson: 'lesson '.repeat(100) });
    const found: string[] = [];
    let input = { query: 'lesson', limit: 5, byte_budget: 3000 };
    for (let page = 0; page < 20; page++) {
      const result = await run('memory.get', input);
      found.push(...result.memories!.map(memory => memory.key));
      expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(3000);
      if (!result.next) break;
      expect(result.next.call.operation).toBe('memory.get');
      input = knowledgeGetSchema.parse(result.next.call.params) as typeof input;
    }
    expect(found).toHaveLength(7);
    expect(new Set(found).size).toBe(7);
    expect(await run('memory.get', { key: 'cache-0', byte_budget: 1024 })).toMatchObject({ partial: true, terminal_limit: { code: 'KNOWLEDGE_ITEM_EXCEEDS_BUDGET' } });
  });

  it('admits one writer after concurrent evidence capture on separate database connections', async () => {
    const { workspace, binding } = fixture();
    const firstDb = new DatabaseSync(join(workspace, 'shared.sqlite'));
    const secondDb = new DatabaseSync(join(workspace, 'shared.sqlite'));
    cleanup.push(() => firstDb.close(), () => secondDb.close());
    initDb(firstDb); initDb(secondDb);
    writeFileSync(join(workspace, 'source.ts'), 'source');
    const original = await executeKnowledgeMemory(firstDb, binding, 'memory.set', entry());
    const expected_revision = original.memory!.revision;
    const results = await Promise.all([firstDb, secondDb].map((db, index) => executeKnowledgeMemory(db,
      { ...binding, actorId: `writer-${index}` }, 'memory.set', {
        ...entry(), expected_revision, request_id: `race-${index}`, why: `writer ${index} rationale`,
        applicability: { files: ['source.ts'], capture_fingerprint: true },
      })));
    expect(results.map(result => result.status).sort()).toEqual(['conflict', 'updated']);
    expect(firstDb.prepare("SELECT COUNT(*) AS n FROM awareness_memories WHERE state = 'ACTIVE'").get()).toEqual({ n: 1 });
    expect(secondDb.prepare('SELECT COUNT(*) AS n FROM awareness_memories').get()).toEqual({ n: 2 });
    const winner = results.find(result => result.status === 'updated')!.memory!;
    expect(results.find(result => result.status === 'conflict')!.current_revision).toBe(winner.revision);
    expect(winner.attribution.actor_id).toMatch(/^writer-[01]$/);
  });

  it('restarts executable pagination when an immutable revision changes the result set', async () => {
    const { run } = fixture();
    for (let index = 0; index < 3; index++) await run('memory.set', entry(`snapshot-${index}`));
    const page = await run('memory.get', { limit: 1 });
    expect(page.next).toBeDefined();
    await run('memory.set', entry('snapshot-new'));
    const changed = await run('memory.get', page.next!.call.params);
    expect(changed).toMatchObject({ status: 'snapshot_changed', partial: true, memories: [], next: { call: { params: { offset: 0 } } } });
    const found: string[] = [];
    let next = changed.next;
    while (next) {
      const result = await run('memory.get', knowledgeGetSchema.parse(next.call.params));
      found.push(...result.memories!.map(memory => memory.key));
      next = result.next;
    }
    expect(new Set(found).size).toBe(4);
    expect((await run('memory.get', { offset: 1 })).status).toBe('snapshot_changed');
  });

  it('keeps context briefings bounded and supplies reads for omissions', async () => {
    const { run, db, binding } = fixture();
    for (let index = 0; index < 5; index++) await run('memory.set', { ...entry(`brief-${index}`), lesson: 'detailed lesson '.repeat(100) });
    const briefing = await getKnowledgeBriefing(db, binding, { files: ['src/cache/a.ts'], limit: 5 });
    expect(Buffer.byteLength(JSON.stringify(briefing))).toBeLessThanOrEqual(1000);
    expect(briefing.partial).toBe(true);
    expect(briefing.next!.call.operation).toBe('memory.get');
    expect((await run('memory.get', knowledgeGetSchema.parse(briefing.next!.call.params))).memories).toHaveLength(5);
  });

  it('keeps declared validity separate from verification and rejects failed capture atomically', async () => {
    const { run, db } = fixture();
    await run('memory.set', { ...entry(), validity: { until: '2000-01-01T00:00:00Z' } });
    expect(await run('memory.get', { key: 'cache' })).toMatchObject({ memories: [{ evidence: { state: 'stale', reason: 'outside_declared_validity', claim_verification: 'unverified' } }] });
    await expect(run('memory.set', { ...entry('missing'), applicability: { files: ['missing.ts'], capture_fingerprint: true } })).rejects.toThrow(/capture memory evidence/i);
    expect(db.prepare('SELECT COUNT(*) AS n FROM awareness_memories').get()).toEqual({ n: 1 });
  });

  it('diagnoses oversized complete continuations and accepts the context file bounds', async () => {
    const { run, db, binding } = fixture();
    await run('memory.set', { ...entry(), anchors: [{ kind: 'directory', value: '.' }] });
    const files = Array.from({ length: 20 }, (_, index) => `src/${'x'.repeat(500)}-${index}.ts`);
    const anchors = files.map(value => ({ kind: 'file' as const, value }));
    expect(knowledgeGetSchema.safeParse({ anchors }).success).toBe(true);
    const restarted = await run('memory.get', { anchors, offset: 1, byte_budget: 1024 });
    expect(restarted).toMatchObject({ partial: true, terminal_limit: { code: 'KNOWLEDGE_CONTINUATION_EXCEEDS_BUDGET' } });
    expect(Buffer.byteLength(JSON.stringify(restarted))).toBeLessThanOrEqual(1024);
    const briefing = await getKnowledgeBriefing(db, binding, { files });
    expect(Buffer.byteLength(JSON.stringify(briefing))).toBeLessThanOrEqual(1000);
    // One compact hit can carry its exact revision without a discovery continuation.
    expect(briefing.memories.length + Number(!!briefing.terminal_limit)).toBeGreaterThan(0);
    for (let index = 0; index < 5; index++) await run('memory.set', { ...entry(`root-${index}`), anchors: [{ kind: 'directory', value: '.' }] });
    const bounded = await getKnowledgeBriefing(db, binding, { files });
    expect(bounded).toMatchObject({ partial: true, terminal_limit: { code: 'KNOWLEDGE_BRIEFING_FILTER_LIMIT' } });
    expect(Buffer.byteLength(JSON.stringify(bounded))).toBeLessThanOrEqual(1000);
  });
});
