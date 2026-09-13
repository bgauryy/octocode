import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync, appendFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { initDb } from '../src/db-init.js';
import { executeKnowledgeMemory, getKnowledgeBriefing } from '../src/knowledge-memory.js';
import { createAwarenessClient } from '../src/client.js';
import type { ExperienceResult } from '../src/experience.js';

const roots: string[] = [];
const databases: DatabaseSync[] = [];
afterEach(() => {
  for (const database of databases.splice(0).reverse()) database.close();
  roots.splice(0).reverse().forEach(root => rmSync(root, { recursive: true, force: true }));
});

function fixture() {
  const workspace = mkdtempSync(join(tmpdir(), 'aw-regression-'));
  roots.push(workspace);
  const database = join(workspace, 'awareness.sqlite3');
  const db = new DatabaseSync(database);
  initDb(db);
  databases.push(db);
  return { workspace, database, db, binding: { workspace, actorId: 'held', sessionId: 'session' } };
}

function knowledgeEntry(key: string, extra: Record<string, unknown> = {}) {
  return {
    key, title: 'Scoped lesson', lesson: 'Use bounded reads', why: 'Avoid dropped evidence',
    anchors: [
      { kind: 'file', value: 'a.ts' }, { kind: 'directory', value: 'src' }, { kind: 'symbol', value: 'fn' },
      { kind: 'flow', value: 'flow' }, { kind: 'failure', value: 'failure' }, { kind: 'task', value: 'task' },
      { kind: 'decision', value: 'decision' },
    ], evidence_refs: ['file:a.ts'], expected_revision: null, request_id: `create-${key}`,
    applicability: { files: ['a.ts'], capture_fingerprint: true }, ...extra,
  };
}

describe('exposed regressions: experience and knowledge', () => {
  it('preserves typed anchors, CAS/retries, workspace isolation, secrets, and stale evidence', async () => {
    const { workspace, db, binding } = fixture();
    writeFileSync(join(workspace, 'a.ts'), 'one');
    const run = (operation: 'memory.set' | 'memory.get' | 'memory.revalidate', input: unknown) =>
      executeKnowledgeMemory(db, binding, operation, input);
    const first = await run('memory.set', knowledgeEntry('k'));
    expect(first.status).toBe('created');
    const revision = first.memory!.revision;
    expect((await run('memory.set', knowledgeEntry('k'))).status).toBe('replayed');
    expect((await run('memory.set', { ...knowledgeEntry('k'), request_id: 'conflict' })).status).toBe('conflict');
    const updated = await run('memory.set', { ...knowledgeEntry('k'), lesson: 'Updated lesson', expected_revision: revision, request_id: 'update' });
    expect(updated.status).toBe('updated');
    expect((await run('memory.get', { key: 'k', revision })).memories?.[0]?.state).toBe('superseded');
    appendFileSync(join(workspace, 'a.ts'), 'two');
    expect((await run('memory.revalidate', { key: 'k' })).review_queue?.[0]?.evidence).toMatchObject({ state: 'stale', claim_verification: 'unverified' });
    const foreign = fixture();
    expect((await executeKnowledgeMemory(db, { ...binding, workspace: foreign.workspace }, 'memory.get', { key: 'k' })).memories).toHaveLength(0);
    await expect(run('memory.set', { ...knowledgeEntry('secret'), lesson: 'api_key=abcdefghi', request_id: 'secret' })).rejects.toThrow(/secret/i);
  });

  it('paginates immutable experience, compares recorded facts, and seals append-only traces', async () => {
    const { workspace, database } = fixture();
    const client = createAwarenessClient({ workspace, database, agentId: 'held', sessionId: 'session' });
    const record = (trace_id: string, event_id: string, outcome: 'failure' | 'success') => client.execute({ operation: 'history.experience', params: {
      action: 'record', trace_id, event_id, title: 'Result', summary: 'Observed result', kind: 'result', outcome,
      anchors: [{ kind: 'decision', value: 'recorded' }], evidence: [{ title: 'fact', text: 'Observed' }],
    } });
    for (let i = 0; i < 5; i++) expect((await record('trace', `event-${i}`, i ? 'success' : 'failure')).exitCode).toBe(0);
    expect((await record('other', 'event', 'success')).exitCode).toBe(0);
    let page = await client.execute({ operation: 'history.experience', params: { action: 'get', trace_id: 'trace', limit: 2 } });
    expect(page.exitCode).toBe(0);
    let payload = page.payload as ExperienceResult;
    const ids: string[] = [];
    const cursors = new Set<string>();
    ids.push(...(payload.events ?? []).map(event => event.event_id));
    while (payload.next) {
      expect(payload.next.operation).toBe('history.experience');
      const cursor = JSON.stringify(payload.next.params);
      expect(cursors.has(cursor)).toBe(false);
      cursors.add(cursor);
      page = await client.execute({ operation: 'history.experience', params: payload.next.params });
      expect(page.exitCode).toBe(0);
      payload = page.payload as ExperienceResult;
      ids.push(...(payload.events ?? []).map(event => event.event_id));
    }
    expect(ids).toEqual(['event-0', 'event-1', 'event-2', 'event-3', 'event-4']);
    expect(new Set(ids).size).toBe(ids.length);
    const comparison = await client.execute({ operation: 'history.experience', params: { action: 'compare', trace_id: 'trace', other_trace_id: 'other' } });
    expect(comparison.exitCode).toBe(0);
    expect((comparison.payload as ExperienceResult).basis).toBe('recorded-facts');
    expect((await client.execute({ operation: 'history.experience', params: { action: 'seal', trace_id: 'trace' } })).exitCode).toBe(0);
    expect((await record('trace', 'after-seal', 'success')).exitCode).toBe(1);
  });

  it('reports bounded briefing terminal limits and executes orientation knowledge continuations', async () => {
    const { workspace, database, db, binding } = fixture();
    for (let i = 0; i < 5; i++) {
      const { applicability: _applicability, ...briefEntry } = knowledgeEntry(`brief-${i}`, { anchors: [{ kind: 'directory', value: '.' }] });
      await executeKnowledgeMemory(db, binding, 'memory.set', { ...briefEntry, lesson: 'lesson '.repeat(100) });
    }
    const files = Array.from({ length: 20 }, (_, i) => `src/${'x'.repeat(500)}${i}.ts`);
    const briefing = await getKnowledgeBriefing(db, binding, { files });
    expect(briefing.partial).toBe(true);
    expect(briefing.terminal_limit?.code).toBe('KNOWLEDGE_BRIEFING_FILTER_LIMIT');
    expect(Buffer.byteLength(JSON.stringify(briefing))).toBeLessThanOrEqual(1000);
    for (let i = 0; i < 4; i++) {
      const { applicability: _applicability, ...flowEntry } = knowledgeEntry(`flow-${i}`, { anchors: [{ kind: 'flow', value: 'pagination' }] });
      await executeKnowledgeMemory(db, binding, 'memory.set', flowEntry);
    }
    const client = createAwarenessClient({ workspace, database, agentId: 'held', sessionId: 'session' });
    const orientation = await client.orient({ flow: 'pagination', limit: 1 });
    expect(orientation).toMatchObject({ partial: true, partialReasons: expect.arrayContaining(['knowledge_partial']) });
    if (orientation.unchanged) throw new Error('expected an orientation payload');
    const continuation = orientation.next.find(call => call.operation === 'memory.get');
    expect(continuation).toBeDefined();
    expect((await client.execute(continuation!)).exitCode).toBe(0);
  });
});
