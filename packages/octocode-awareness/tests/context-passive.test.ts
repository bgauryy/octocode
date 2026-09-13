import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { CONTINUITY_SCHEMA_DDL } from '../src/db-continuity-schema.js';
import { assessContextRegulation, observeContext, recordContextFeedback } from '../src/context-regulation.js';

const scope = { workspace: '/test', actorId: 'any-agent', sessionId: 'run' };
const now = '2026-09-12T12:00:00.000Z';
const clock = { now };
let db: DatabaseSync;
beforeEach(() => { db = new DatabaseSync(':memory:'); db.exec(CONTINUITY_SCHEMA_DDL); });
afterEach(() => db.close());
const sample = (id: string, used = 95) => ({ observation_id: id, observed_at: now,
  acquisition: 'passive' as const, source: 'host' as const, context: { used, limit: 100 } });

it('offers one passive nudge per pressure episode; retries preserve its identity', () => {
  const first = observeContext(db, scope, sample('one'), clock);
  expect(first).toMatchObject({ run_state: { status: 'pressured' }, nudge: { kind: 'context-pressure', advisory: true } });
  expect(observeContext(db, scope, sample('one'), clock).nudge).toEqual(first.nudge);
  expect(observeContext(db, scope, sample('two'), clock)).not.toHaveProperty('nudge');
  observeContext(db, scope, sample('recovered', 40), clock);
  const recurrence = observeContext(db, scope, sample('again'), clock);
  expect(recurrence.nudge?.advisory_id).not.toBe(first.nudge?.advisory_id);
});

it('keeps interpreted pressure after advice is dismissed and marks stale state unknown', () => {
  const first = observeContext(db, scope, sample('one'), clock);
  recordContextFeedback(db, scope, { feedback_id: 'dismiss', observed_at: now,
    advisory_id: first.nudge!.advisory_id, action_taken: 'Enough room for remaining work', outcome: 'unnecessary' }, clock);
  const current = assessContextRegulation(db, scope, clock);
  expect(current.advisories).toEqual([]);
  expect(current.run_state.status).toBe('pressured');
  expect(assessContextRegulation(db, scope, { now: '2026-09-12T12:06:00.000Z' }).run_state.status).toBe('unknown');
});

it('separates active reports from passive offers and does not offer historical pressure', () => {
  expect(observeContext(db, scope, { ...sample('active'), acquisition: 'active' }, clock)).not.toHaveProperty('nudge');
  expect(assessContextRegulation(db, scope, clock).run_state.status).toBe('pressured');
  expect(observeContext(db, { ...scope, sessionId: 'history' }, sample('old'), { now: '2026-09-12T12:06:00.000Z' })).not.toHaveProperty('nudge');
});

it('distinguishes reported progress from mere successful execution', () => {
  observeContext(db, scope, { observation_id: 'baseline', observed_at: now, progress: { scope: 'verified-milestones', value: 0 } }, clock);
  observeContext(db, scope, { observation_id: 'advance', observed_at: now, progress: { scope: 'verified-milestones', value: 1 } }, clock);
  expect(assessContextRegulation(db, scope, clock).run_state.status).toBe('progressing');
  observeContext(db, scope, { observation_id: 'success', observed_at: now, action_fingerprint: 'call', outcome: 'success' }, clock);
  expect(assessContextRegulation(db, scope, clock).run_state.status).toBe('observed');
});

it('does not treat a missing context sample as recovery that re-arms pressure', () => {
  observeContext(db, scope, sample('pressure'), clock);
  observeContext(db, scope, { observation_id: 'tool-only', observed_at: now, acquisition: 'passive',
    action_fingerprint: 'test', outcome: 'success' }, clock);
  expect(assessContextRegulation(db, scope, clock).unavailable).toContain('context');
  expect(observeContext(db, scope, sample('still-pressure'), clock)).not.toHaveProperty('nudge');
});
