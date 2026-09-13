import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAwarenessClient } from '../src/client.js';
import { executeAwarenessCli } from '../src/command-cli.js';
import type { ContextObservation } from '../src/context-regulation.js';

const roots: string[] = [];
afterEach(() => { vi.useRealTimers(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-lifecycle-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'aw.sqlite3'), agentId: 'generic-agent', sessionId: 'task-session' };
}
const now = '2026-09-12T12:00:00.000Z';
function report(index: number): ContextObservation {
  return { observation_id: `sample-${index}`, observed_at: now, action_fingerprint: 'search:signature',
    outcome: 'failure', evidence_revision: 'source-v1', progress: { scope: 'verified-milestones', value: 0 },
    context: { used: 950, limit: 1000 } };
}

it('runs an agnostic CLI/API observation, advice, action, and outcome loop', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now);
  const context = fixture();
  const client = createAwarenessClient(context);
  const args = ['--db', context.database, '--workspace', context.workspace, '--agent-id', context.agentId, '--session-id', context.sessionId];
  const sample = report(0);
  const cli = await executeAwarenessCli(['context', 'observe', ...args,
    '--observation-id', sample.observation_id, '--observed-at', sample.observed_at,
    '--action-fingerprint', sample.action_fingerprint!, '--outcome', sample.outcome!,
    '--evidence-revision', sample.evidence_revision!, '--progress', JSON.stringify(sample.progress), '--context', JSON.stringify(sample.context)]);
  expect(cli.exitCode, JSON.stringify(cli.payload)).toBe(0);
  expect(await client.observe(sample)).toMatchObject({ exitCode: 0, payload: { replayed: true } });
  for (let i = 1; i < 3; i++) expect((await client.observe(report(i))).exitCode).toBe(0);
  const first = await client.orient();
  if (first.unchanged) throw new Error('expected initial packet');
  expect(first.regulation.advisories?.map(a => a.kind)).toContain('repetition');
  expect(first).not.toHaveProperty('recovery');
  expect(Buffer.byteLength(JSON.stringify(first))).toBeLessThanOrEqual(6_000);
  expect((await executeAwarenessCli(['context', 'orient', ...args])).payload).toEqual(first);
  expect(await client.orient({ if_revision: first.revision })).toEqual({ revision: first.revision, unchanged: true });
  const pressure = first.regulation.advisories!.find(a => a.kind === 'context-pressure')!;
  const subsequent = { ...report(3), outcome: 'success' as const, evidence_revision: 'source-v2',
    progress: { scope: 'verified-milestones', value: 1 }, context: { used: 400, limit: 1000 } };
  expect((await client.observe(subsequent)).exitCode).toBe(0);
  expect((await client.feedback({ feedback_id: 'compaction', observed_at: now, advisory_id: pressure.id,
    action_taken: 'checkpointed and compacted', outcome: 'helpful', observation_id: subsequent.observation_id })).exitCode).toBe(0);
  const improved = await client.orient({ if_revision: first.revision });
  expect(improved).toMatchObject({ unchanged: false, operational: { runtime: { usefulness: { helpful: 1, basis: 'reported-feedback' } } } });
  if (improved.unchanged) throw new Error('expected changed packet');
  expect(improved.regulation.advisories ?? []).toEqual([]);
  expect(improved.verification.pending).toBe(0);
});

it('invalidates stale observations without a write and isolates another session', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now);
  const context = fixture(); const client = createAwarenessClient(context);
  await client.observe(report(0));
  const first = await client.orient();
  vi.setSystemTime(Date.parse(now) + 300_001);
  const expired = await client.orient({ if_revision: first.revision });
  expect(expired).toMatchObject({ unchanged: false, operational: { runtime: { freshness: 'stale' } } });
  if (expired.unchanged) throw new Error('expected stale transition');
  expect(expired.regulation.advisories ?? []).toEqual([]);
  expect(await createAwarenessClient({ ...context, sessionId: 'other' }).orient()).not.toHaveProperty('operational.runtime');
});

it('requires session bindings, rejects fabricated helpful outcomes and invalid object flags', async () => {
  const context = fixture();
  expect((await createAwarenessClient({ ...context, sessionId: undefined }).observe(report(0))).exitCode).toBe(1);
  expect((await createAwarenessClient(context).execute({ operation: 'context.feedback', params: {
    feedback_id: 'invented', advisory_id: 'invented', observed_at: now, action_taken: 'none', outcome: 'helpful',
  } })).exitCode).toBe(1);
  expect((await executeAwarenessCli(['context', 'observe', '--agent-id', 'agent', '--progress', '{broken'])).exitCode).toBe(1);
});

it('keeps all advice reachable at the smallest workspace page with maximal receipt IDs', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now);
  const context = fixture(); const client = createAwarenessClient(context);
  for (let i = 0; i < 3; i++) await client.observe({ ...report(i), observation_id: `${i}${'a'.repeat(127)}`,
    progress: { scope: '🧪'.repeat(100), value: 0 } });
  for (let i = 0; i < 3; i++) {
    const sent = await createAwarenessClient({ ...context, agentId: `peer-${i}` }).execute({
      operation: 'message.send', params: { kind: 'question', subject: 'long subject '.repeat(10), body: 'long body '.repeat(100), to_agent: [context.agentId] },
    });
    expect(sent.exitCode).toBe(0);
  }
  const result = await client.execute({ operation: 'context.orient', params: { limit: 1 } });
  expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
  expect(result.payload).toMatchObject({ regulation: { advisories: expect.arrayContaining([
    expect.objectContaining({ kind: 'repetition' }), expect.objectContaining({ kind: 'stalled-progress' }),
    expect.objectContaining({ kind: 'context-pressure' }), expect.objectContaining({ kind: 'tool-failure' }),
  ]) } });
  expect(Buffer.byteLength(JSON.stringify(result.payload))).toBeLessThanOrEqual(6_000);
});

it('accepts context-only CLI observations with honest unavailable tool sensors', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now);
  const context = fixture();
  const result = await executeAwarenessCli(['context', 'observe', '--db', context.database, '--workspace', context.workspace,
    '--agent-id', context.agentId, '--session-id', context.sessionId, '--observation-id', 'context-only', '--observed-at', now,
    '--source', 'host', '--context', '{"used":950,"limit":1000}']);
  expect(result.exitCode, JSON.stringify(result.payload)).toBe(0);
  const packet = await createAwarenessClient(context).orient();
  expect(packet).toMatchObject({ operational: { unavailable: expect.arrayContaining(['tool_health']), runtime: {
    source: 'host', context: { used: 950, limit: 1000 }, unavailable: expect.arrayContaining(['tool_health']),
  } } });
});

it('offers passive CLI nudges without requiring an extra orientation call', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now);
  const context = fixture();
  const args = ['context', 'observe', '--db', context.database, '--workspace', context.workspace,
    '--agent-id', context.agentId, '--session-id', context.sessionId, '--observed-at', now,
    '--acquisition', 'passive', '--context', '{"used":950,"limit":1000}'];
  const first = await executeAwarenessCli([...args, '--observation-id', 'passive-one']);
  expect(first.exitCode, JSON.stringify(first.payload)).toBe(0);
  expect(first.payload).toMatchObject({ run_state: { status: 'pressured' }, nudge: { kind: 'context-pressure' } });
  expect(Buffer.byteLength(JSON.stringify(first.payload))).toBeLessThanOrEqual(1_500);
  const second = await executeAwarenessCli([...args, '--observation-id', 'passive-two']);
  expect(second.exitCode).toBe(0);
  expect(second.payload).not.toHaveProperty('nudge');
  expect(await createAwarenessClient(context).orient()).toMatchObject({ run_state: { status: 'pressured' },
    regulation: { nudge: { kind: 'context-pressure' } } });
});
