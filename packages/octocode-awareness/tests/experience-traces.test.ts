import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { CONTINUITY_SCHEMA_DDL } from '../src/db-continuity-schema.js';
import { executeExperience } from '../src/experience.js';
import { appendDomainEvent } from '../src/event-outbox.js';
import { getAwarenessOperationDescriptor } from '../src/schema/operation-catalog.js';

let db: DatabaseSync;
const binding = { workspace: '/test/workspace', actorId: 'agent', sessionId: 'session' };
const record = (trace_id: string, event_id: string, extra = {}) => ({ action: 'record' as const,
  trace_id, event_id, title: 'Experiment', summary: 'Observed response', kind: 'attempt' as const, ...extra });
beforeEach(() => { db = new DatabaseSync(':memory:'); db.exec(CONTINUITY_SCHEMA_DDL); });
afterEach(() => db.close());

describe('bounded immutable experience traces', () => {
  it('records no-file experiments, preserves provenance, and rejects ID conflicts', async () => {
    const input = record('trace', 'one');
    const first = await executeExperience(db, binding, input);
    expect(await executeExperience(db, binding, input)).toEqual(first);
    await expect(executeExperience(db, binding, record('trace', 'one', { summary: 'Changed' }))).rejects.toThrow(/conflict/i);
    await expect(executeExperience(db, { ...binding, actorId: 'other' }, input)).rejects.toThrow(/conflict/i);
    const result = await executeExperience(db, binding, { action: 'get', trace_id: 'trace' });
    expect(result.events).toEqual([expect.objectContaining({ event_id: 'one', actor_id: 'agent', session_id: 'session', anchors: [] })]);
    expect(db.prepare('SELECT retention_class FROM event_outbox').get()?.retention_class).toBe('audit');
  });
  it('rejects secrets throughout content and enforces event size bounds before writes', async () => {
    for (const extra of [ { title: 'password=abcdefghi' }, { summary: 'api_key=abcdefghi' },
      { rationale: 'access_token=abcdefghi' }, { evidence: [{ title: 'check', text: 'client_secret=abcdefghi' }] },
      { anchors: [{ kind: 'failure', value: 'password=abcdefghi' }] } ]) {
      await expect(executeExperience(db, binding, record('trace', 'secret', extra))).rejects.toThrow(/secret/i);
    }
    await expect(executeExperience(db, binding, record('trace', 'large', { summary: 'x'.repeat(2000) }))).rejects.toThrow();
    expect(db.prepare('SELECT count(*) AS n FROM event_outbox').get()?.n).toBe(0);
  });
  it('isolates workspaces and follows stable executable event continuations across appends', async () => {
    for (let i = 0; i < 5; i++) await executeExperience(db, binding, record('trace', `e${i}`));
    await executeExperience(db, { ...binding, workspace: '/other' }, record('trace', 'private'));
    let page = await executeExperience(db, binding, { action: 'get', trace_id: 'trace', limit: 2 });
    const ids = page.events!.map((event: { event_id: string }) => event.event_id);
    await executeExperience(db, binding, record('trace', 'later'));
    while (page.next) {
      expect(page.next.operation).toBe('history.experience');
      page = await executeExperience(db, binding, page.next.params);
      ids.push(...page.events!.map((event: { event_id: string }) => event.event_id));
    }
    expect(ids).toEqual(['e0', 'e1', 'e2', 'e3', 'e4']);
    expect((await executeExperience(db, { ...binding, workspace: '/other' }, { action: 'get', trace_id: 'trace' })).events).toHaveLength(1);
  });
  it('freezes sealed SQLite traces even when an archive cannot be created, then exposes recovery', async () => {
    await executeExperience(db, binding, record('trace', 'e1', { outcome: 'failure' }));
    const result = await executeExperience(db, binding, { action: 'seal', trace_id: 'trace' });
    expect(result.archive!.status).toBe('unavailable');
    expect(result.archive!.durable).toBe(false);
    await expect(executeExperience(db, binding, record('trace', 'e2'))).rejects.toThrow(/sealed/i);
    expect((await executeExperience(db, binding, { action: 'recover' })).traces).toEqual([
      expect.objectContaining({ trace_id: 'trace', state: 'sealed', archive_status: 'pending' }),
    ]);
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'trace' })).events).toHaveLength(1);
  });
  it('lists losslessly and compares factual outcomes without invented causes', async () => {
    for (let i = 0; i < 4; i++) await executeExperience(db, binding, record(`t${i}`, 'e', { outcome: i ? 'success' : 'failure' }));
    let page = await executeExperience(db, binding, { action: 'list', limit: 2 });
    const ids = page.traces!.map((trace: { trace_id: string }) => trace.trace_id);
    while (page.next) { page = await executeExperience(db, binding, page.next.params); ids.push(...page.traces!.map((trace: { trace_id: string }) => trace.trace_id)); }
    expect(ids).toEqual(['t0', 't1', 't2', 't3']);
    const comparison = await executeExperience(db, binding, { action: 'compare', trace_id: 't0', other_trace_id: 't1' });
    expect(comparison.basis).toBe('recorded-facts');
    expect(comparison.differences).toEqual([expect.objectContaining({ left: expect.objectContaining({ outcome: 'failure' }), right: expect.objectContaining({ outcome: 'success' }) })]);
  });
  it('enforces trace and UTF-8 byte bounds without silently losing accepted events', async () => {
    await expect(executeExperience(db, binding, record('large', 'utf8', {
      evidence: Array.from({ length: 4 }, () => ({ title: 'Bytes', text: '😀'.repeat(512) })),
    }))).rejects.toThrow(/8192 bytes/);
    for (let i = 0; i < 128; i++) await executeExperience(db, binding, record('full', `e${i}`));
    await expect(executeExperience(db, binding, record('full', 'overflow'))).rejects.toThrow(/128 events/);
    expect((await executeExperience(db, binding, record('full', 'e0'))).event?.event_id).toBe('e0');
    let page = await executeExperience(db, binding, { action: 'get', trace_id: 'full', limit: 10 });
    const ids = page.events!.map(event => event.event_id);
    while (page.next) { page = await executeExperience(db, binding, page.next.params); ids.push(...page.events!.map(event => event.event_id)); }
    expect(ids).toEqual(Array.from({ length: 128 }, (_, i) => `e${i}`));
  });
  it('keeps large pages within output budget with executable byte-limit continuations', async () => {
    const extra = { rationale: 'r'.repeat(1000), summary: 's'.repeat(1000),
      evidence: Array.from({ length: 4 }, () => ({ title: 'Output', text: 'e'.repeat(1024) })) };
    for (let i = 0; i < 10; i++) await executeExperience(db, binding, record('bytes', `e${i}`, extra));
    let page = await executeExperience(db, binding, { action: 'get', trace_id: 'bytes', limit: 10 });
    expect(page.partial).toBe(true);
    expect(getAwarenessOperationDescriptor('history.experience')?.outputBudget).toBeLessThanOrEqual(32 * 1024);
    const ids: string[] = [];
    do {
      expect(Buffer.byteLength(JSON.stringify(page))).toBeLessThan(32 * 1024);
      ids.push(...page.events!.map(event => event.event_id));
      if (!page.next) break;
      page = await executeExperience(db, binding, page.next.params);
    } while (true);
    expect(ids).toEqual(Array.from({ length: 10 }, (_, i) => `e${i}`));
  });
  it('rejects cursor reuse across workspace, trace, and source selections', async () => {
    for (let i = 0; i < 3; i++) await executeExperience(db, binding, record('cursor', `e${i}`));
    const page = await executeExperience(db, binding, { action: 'get', trace_id: 'cursor', limit: 1 });
    const params = page.next!.params;
    await expect(executeExperience(db, { ...binding, workspace: '/other' }, params)).rejects.toThrow(/CURSOR_INVALID/);
    await expect(executeExperience(db, binding, { ...params, trace_id: 'other' })).rejects.toThrow(/CURSOR_INVALID/);
    await expect(executeExperience(db, binding, { ...params, source: 'archive' })).rejects.toThrow(/CURSOR_INVALID/);
    await expect(executeExperience(db, binding, { ...params, cursor: Buffer.from('null').toString('base64url') })).rejects.toThrow(/CURSOR_INVALID/);
  });
  it('normalizes scoped anchors and rejects escaping paths and control sequences', async () => {
    const result = await executeExperience(db, binding, record('anchored', 'e', {
      anchors: [{ kind: 'file', value: './src/a.ts' }, { kind: 'directory', value: 'src' }, { kind: 'flow', value: 'request-to-retry' }],
    }));
    expect(result.event?.anchors).toContainEqual({ kind: 'file', value: 'src/a.ts' });
    await expect(executeExperience(db, binding, record('anchored', 'escape', { anchors: [{ kind: 'file', value: '../../escape' }] }))).rejects.toThrow(/workspace/);
    await expect(executeExperience(db, binding, record('anchored', 'control', { evidence: [{ title: 'Output', text: '\u001b[31mred' }] }))).rejects.toThrow(/Control/);
  });
  it('compares paginated differences only and freezes recover pages across later seals', async () => {
    for (let i = 0; i < 5; i++) {
      await executeExperience(db, binding, record('left', `e${i}`, { outcome: 'failure' }));
      await executeExperience(db, binding, record('right', `e${i}`, { outcome: i === 2 ? 'failure' : 'success' }));
    }
    let page = await executeExperience(db, binding, { action: 'compare', trace_id: 'left', other_trace_id: 'right', limit: 2 });
    const ordinals = page.differences!.map(difference => difference.ordinal);
    while (page.next) { page = await executeExperience(db, binding, page.next.params); ordinals.push(...page.differences!.map(difference => difference.ordinal)); }
    expect(ordinals).toEqual([0, 1, 3, 4]);
    expect((await executeExperience(db, binding, { action: 'compare', trace_id: 'left', other_trace_id: 'left' })).differences).toEqual([]);
    const recovering = await executeExperience(db, binding, { action: 'recover', limit: 1 });
    await executeExperience(db, binding, { action: 'seal', trace_id: 'right' });
    const continued = await executeExperience(db, binding, recovering.next!.params);
    expect(continued.traces).toEqual([expect.objectContaining({ trace_id: 'right', state: 'open' })]);
  });
  it('ignores unrelated aggregate events and reports a terminal bound for oversized journals', async () => {
    await executeExperience(db, binding, record('bounded', 'first'));
    for (let i = 0; i < 132; i++) appendDomainEvent(db, { workspace: binding.workspace, actorId: binding.actorId,
      eventType: 'host.custom', aggregateKind: 'experience', aggregateId: 'bounded', retentionClass: 'audit',
      createdAt: new Date().toISOString(), payload: { unrelated: true } });
    await executeExperience(db, binding, record('bounded', 'last'));
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'bounded' })).events?.map(event => event.event_id)).toEqual(['first', 'last']);
    for (let i = 0; i < 129; i++) appendDomainEvent(db, { workspace: binding.workspace, actorId: binding.actorId,
      eventType: 'experience.record', aggregateKind: 'experience', aggregateId: 'bounded', retentionClass: 'audit',
      createdAt: new Date().toISOString(), payload: record('bounded', `injected${i}`) });
    await expect(executeExperience(db, binding, { action: 'get', trace_id: 'bounded' })).rejects.toMatchObject({
      code: 'EXPERIENCE_JOURNAL_LIMIT', terminal_limit: { kind: 'trace-journal', maximum: 130 },
    });
  });
});
