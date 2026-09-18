import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CONTINUITY_SCHEMA_DDL } from '../src/db-continuity-schema.js';
import { assessContextRegulation, observeContext, recordContextFeedback } from '../src/context-regulation.js';

const scope = { workspace: '/test/workspace', actorId: 'agent', sessionId: 'session' };
const at = '2026-09-12T12:00:00.000Z';
const report = (id: string, changes = {}) => ({ observation_id: id, observed_at: at,
  action_fingerprint: 'opaque-hash', outcome: 'success' as const, evidence_revision: 'rev',
  progress: { scope: 'task', value: 0 }, ...changes });
let db: DatabaseSync;
beforeEach(() => { db = new DatabaseSync(':memory:'); db.exec(CONTINUITY_SCHEMA_DDL); });
afterEach(() => db.close());
const assess = () => assessContextRegulation(db, scope, { now: at });
describe('attributed context regulation', () => {
  it('rejects oversized or nonopaque observation and feedback identifiers', () => {
    for (const id of ['x'.repeat(129), 'unicode-שלום', 'has spaces']) {
      expect(() => observeContext(db, scope, report(id))).toThrow();
      expect(() => recordContextFeedback(db, scope, { feedback_id: id, observed_at: at,
        advisory_id: 'unknown', action_taken: 'inspect', outcome: 'unresolved' })).toThrow();
    }
  });
  it('keeps handled dispositions across feedback rollover using indexed point lookups', () => {
    observeContext(db, scope, report('first', { outcome: 'failure' }));
    const advisory = assess().advisories[0]!;
    recordContextFeedback(db, scope, { feedback_id: 'handled', observed_at: at,
      advisory_id: advisory.id, action_taken: 'inspect', outcome: 'unnecessary' });
    for (let i = 0; i < 70; i++) recordContextFeedback(db, scope, { feedback_id: `f${i}`, observed_at: at,
      advisory_id: advisory.id, action_taken: 'inspect', outcome: 'unresolved' });
    const prepare = vi.spyOn(db, 'prepare');
    expect(assess().advisories).toEqual([]);
    expect(prepare.mock.calls.filter(([sql]) => sql === 'SELECT 1 FROM event_outbox WHERE event_id = ?')).toHaveLength(1);
    prepare.mockRestore();
    expect(db.prepare("SELECT COUNT(*) AS n FROM event_outbox WHERE event_type = 'context.advisory-handled'").get()?.n).toBe(1);
    expect(assess().coverage.terminal_limit?.kind).toBe('assessment-window');
  });
  it('accepts context-only reports without inventing action or progress evidence', () => {
    for (let i = 0; i < 3; i++) observeContext(db, scope, { observation_id: String(i), observed_at: at,
      context: { used: 95, limit: 100 } });
    expect(assess().advisories.map(a => a.kind)).toEqual(['context-pressure']);
    expect(assess().unavailable).toEqual(expect.arrayContaining(['progress', 'tool_health', 'repetition']));
    expect(() => observeContext(db, scope, { observation_id: 'empty', observed_at: at })).toThrow();
    expect(() => observeContext(db, scope, { observation_id: 'partial', observed_at: at,
      action_fingerprint: 'action', context: { used: 1, limit: 10 } })).toThrow();
    expect(() => observeContext(db, scope, { observation_id: 'partial2', observed_at: at, outcome: 'success' })).toThrow();
  });
  it('requires evidence revision for progress comparison and never infers progress from success', () => {
    for (let i = 0; i < 3; i++) observeContext(db, scope, { observation_id: String(i), observed_at: at,
      action_fingerprint: 'same', outcome: 'success' });
    expect(assess().advisories).toEqual([]);
    expect(assess().unavailable).toContain('progress');
    for (let i = 3; i < 6; i++) observeContext(db, scope, { observation_id: String(i), observed_at: at,
      progress: { scope: 'task', value: 0 } });
    expect(assess().advisories).toEqual([]);
    expect(assess().unavailable).toContain('stalled_progress');
    for (let i = 6; i < 9; i++) observeContext(db, scope, { observation_id: String(i), observed_at: at,
      progress: { scope: 'task', value: 0 }, evidence_revision: 'known' });
    expect(assess().advisories.map(a => a.kind)).toEqual(['stalled-progress']);
  });
  it('resets repetition after a sample without action evidence', () => {
    observeContext(db, scope, report('1')); observeContext(db, scope, report('2')); observeContext(db, scope, report('3'));
    expect(assess().advisories.map(a => a.kind)).toContain('repetition');
    observeContext(db, scope, { observation_id: 'context', observed_at: at, context: { used: 10, limit: 100 } });
    observeContext(db, scope, report('4'));
    expect(assess().advisories.map(a => a.kind)).not.toContain('repetition');
  });
  it('detects repeated unchanged attempts, not duplicate deliveries, and isolates identity', () => {
    observeContext(db, scope, report('1'));
    observeContext(db, scope, report('1'));
    observeContext(db, { ...scope, actorId: 'other' }, report('2'));
    expect(assess().advisories).toHaveLength(0);
    observeContext(db, scope, report('2'));
    observeContext(db, scope, report('3'));
    expect(assess().advisories.map(a => a.kind)).toContain('repetition');
    expect(assess().advisories.find(a => a.kind === 'repetition')?.observation_ids).toEqual(['1', '2', '3']);
    expect(() => observeContext(db, scope, report('3', { outcome: 'failure' }))).toThrow();
  });
  it('resets repetition for changed evidence, progress, action and metric scope', () => {
    for (const changes of [{ evidence_revision: 'new' }, { progress: { scope: 'task', value: 1 } },
      { action_fingerprint: 'another' }, { progress: { scope: 'other', value: 0 } }]) {
      const actor = { ...scope, actorId: JSON.stringify(changes) };
      observeContext(db, actor, report('1')); observeContext(db, actor, report('2'));
      observeContext(db, actor, report('3', changes));
      expect(assessContextRegulation(db, actor, { now: at }).advisories.map(a => a.kind)).not.toContain('repetition');
    }
  });
  it('reports missing sensors and expires old observations without recovery claims', () => {
    expect(assess().unavailable).toContain('context');
    observeContext(db, scope, report('1', { context: { used: 95, limit: 100 } }));
    expect(assess().advisories.map(a => a.kind)).toContain('context-pressure');
    expect(assessContextRegulation(db, scope, { now: '2026-09-12T12:06:00.000Z' }).advisories).toEqual([]);
  });
  it('accepts only derived advisory feedback and suppresses handled episodes until recovery', () => {
    observeContext(db, scope, report('1', { context: { used: 95, limit: 100 } }));
    const advisory = assess().advisories[0]!;
    expect(() => recordContextFeedback(db, scope, { feedback_id: 'too-early', observed_at: '2026-09-12T11:59:00.000Z',
      advisory_id: advisory.id, action_taken: 'inspect', outcome: 'unresolved' })).toThrow();
    observeContext(db, scope, report('result', { context: { used: 95, limit: 100 } }));
    const feedback = { feedback_id: 'f1', observed_at: at, advisory_id: advisory.id, observation_id: 'result',
      action_taken: 'checkpoint', outcome: 'helpful' as const };
    expect(() => recordContextFeedback(db, { ...scope, sessionId: 'other' }, feedback)).toThrow();
    expect(() => recordContextFeedback(db, scope, { ...feedback, advisory_id: 'unknown' })).toThrow();
    recordContextFeedback(db, scope, feedback); recordContextFeedback(db, scope, feedback);
    observeContext(db, scope, report('2', { context: { used: 99, limit: 100 } }));
    expect(assess().advisories.map(a => a.kind)).not.toContain('context-pressure');
    expect(assess().usefulness.helpful).toBe(1);
    observeContext(db, scope, report('3', { context: { used: 10, limit: 100 } }));
    observeContext(db, scope, report('4', { context: { used: 95, limit: 100 } }));
    expect(assess().advisories.find(a => a.kind === 'context-pressure')?.id).not.toBe(advisory.id);
  });
  it('declares bounded coverage and preserves episode ids across window rollover', () => {
    observeContext(db, scope, report('0', { outcome: 'failure' }));
    const first = assess().advisories[0]!.id;
    for (let i = 1; i < 70; i++) observeContext(db, scope, report(String(i), { outcome: 'failure' }));
    expect(assess().coverage.terminal_limit?.kind).toBe('assessment-window');
    expect(assess().advisories.find(a => a.kind === 'tool-failure')?.id).toBe(first);
  });
  it('requires a later result observation for helpful feedback and rejects invented fields', () => {
    observeContext(db, scope, report('first', { outcome: 'failure', source: 'host' }));
    const advisory = assess().advisories[0]!;
    const feedback = { feedback_id: 'f', observed_at: at, advisory_id: advisory.id,
      action_taken: 'retry', outcome: 'helpful' as const };
    expect(() => recordContextFeedback(db, scope, feedback)).toThrow();
    expect(() => recordContextFeedback(db, scope, { ...feedback, observation_id: 'first' })).toThrow();
    observeContext(db, { ...scope, workspace: '/other' }, report('result'));
    expect(() => recordContextFeedback(db, scope, { ...feedback, observation_id: 'result' })).toThrow();
    observeContext(db, scope, report('result'));
    recordContextFeedback(db, scope, { ...feedback, observation_id: 'result' });
    const count = db.prepare('SELECT COUNT(*) AS n FROM event_outbox').get();
    assess(); assess();
    expect(db.prepare('SELECT COUNT(*) AS n FROM event_outbox').get()).toEqual(count);
    expect(() => observeContext(db, scope, report('bad', { context: { used: 1, limit: 0 } }))).toThrow();
    expect(() => recordContextFeedback(db, scope, { ...feedback, observation_id: 'result', verified: true } as never)).toThrow();
  });
  it('leaves unresolved advisories visible and resets episodes after freshness gaps', () => {
    observeContext(db, scope, report('first', { outcome: 'failure' }));
    const advisory = assess().advisories[0]!;
    recordContextFeedback(db, scope, { feedback_id: 'f', observed_at: at, advisory_id: advisory.id,
      action_taken: 'inspect', outcome: 'unresolved' });
    expect(assess().advisories[0]!.id).toBe(advisory.id);
    observeContext(db, scope, report('later', { observed_at: '2026-09-12T12:06:00.000Z', outcome: 'failure' }));
    expect(assessContextRegulation(db, scope, { now: '2026-09-12T12:06:00.000Z' }).advisories[0]!.id).not.toBe(advisory.id);
    expect(() => observeContext(db, scope, report('backdated'))).toThrow();
    expect(() => observeContext(db, scope, report('future', { observed_at: '2099-09-12T12:06:00.000Z' }), { now: at })).toThrow();
  });
});
