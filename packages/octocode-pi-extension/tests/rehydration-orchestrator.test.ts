import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { contentDigest } from '@octocodeai/octocode-awareness';
import { createSessionArtifactContext, writeRehydrationLedger } from '../src/tools/session-artifacts.js';
import { REHYDRATION_RECEIPT_ENTRY_TYPE, consumeValidatedRehydration, rehydrateSession, runAndRecordRehydration } from '../src/tools/rehydration-orchestrator.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => fs.rmSync(root, { recursive: true, force: true })));

function setup(sessionId = 'session-1') {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-flow-'));
  roots.push(workspace);
  const ctx = { cwd: workspace, mode: 'tui', sessionManager: { getSessionId: () => sessionId } } as const;
  const artifact = createSessionArtifactContext(ctx);
  return { workspace, ctx, artifact };
}

describe('production rehydration orchestration', () => {
  it('keeps resumed work stopped while durable user input is unresolved', () => {
    const { ctx } = setup();
    const setActivity = vi.fn();
    rehydrateSession(ctx as never, 'resume', {
      getLivePlan: () => ({
        scope: 'live',
        branchSnapshotId: 'snapshot',
        generation: 1,
        phase: 'executing',
        content: '<active_plan>work</active_plan>',
        pendingInteractionIds: ['interaction-1'],
      }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity,
    });
    expect(setActivity).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'awaiting_input',
      question: expect.stringContaining('interaction-1'),
    }));
  });

  it('restores digest-valid bounded refs while live plan, pending interactions, and cursors win', () => {
    const { workspace, ctx, artifact } = setup();
    const currentPlan = '<active_plan>live</active_plan>';
    const policy = 'frozen policy';
    writeRehydrationLedger(artifact, {
      capturedAt: '2026-08-26T00:00:00.000Z',
      segments: [
        { version: 1, id: 'active-plan', kind: 'plan', origin: 'plan-domain', authority: 'user', digest: contentDigest(currentPlan), scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 20 },
        { version: 1, id: 'policy', kind: 'product-policy', origin: 'octocode-harness', authority: 'product', digest: contentDigest(policy), scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20 },
      ],
      segmentContents: { 'active-plan': currentPlan, policy },
      plan: { scope: `${workspace}\0id:session-1`, branchSnapshotId: 'snapshot-2', generation: 2, revision: 'r2' },
      pendingInteractionIds: ['gone', 'pending'],
      consumerCursors: { tui: 4, rpc: 99 },
    });
    const setActivity = vi.fn();
    const receipt = rehydrateSession(ctx as never, 'resume', {
      getLivePlan: () => ({ scope: `${workspace}\0id:session-1`, branchSnapshotId: 'snapshot-2', generation: 2, revision: 'r2', phase: 'executing', content: currentPlan }),
      openContinuity: () => ({
        listPendingInteractions: () => [{ request: { interactionId: 'pending' } }],
        getConsumerCursor: (id: string) => id === 'tui' ? 7 : 3,
        close: vi.fn(),
      }),
      setActivity,
      now: () => Date.parse('2026-08-26T00:01:00.000Z'),
      totalTokenBudget: 100,
    });
    expect(receipt.outcome).toBe('pending-validation');
    expect(receipt.present).toEqual(['active-plan', 'policy']);
    expect(receipt.restored).toEqual([]);
    expect(receipt.pendingInteractionIds).toEqual(['pending']);
    expect(receipt.consumerCursors).toEqual({ rpc: { live: 3, hint: 99, decision: 'held' }, tui: { live: 7, hint: 4, decision: 'live-ahead' } });
    expect(setActivity).toHaveBeenCalledWith(expect.objectContaining({ kind: 'working' }));
    const projection = consumeValidatedRehydration(ctx as never, [
      { segment: { ...writeSegment('active-plan', 'plan', 'plan-domain', 'user', 'task', 'transcript', currentPlan) }, content: currentPlan },
      { segment: { ...writeSegment('policy', 'product-policy', 'octocode-harness', 'product', 'session', 'hidden-policy', policy) }, content: policy },
    ], {
      allowProjection: true,
      totalTokenBudget: 100,
      now: () => Date.parse('2026-08-26T00:01:00.000Z'),
    });
    expect(projection?.receipt.validated).toEqual(['active-plan', 'policy']);
    expect(projection?.receipt.reprojected).toEqual(['active-plan']);
    expect(projection?.content).toContain(currentPlan);
    expect(projection?.content).not.toContain(policy);
    expect(consumeValidatedRehydration(ctx as never, [], { allowProjection: true })).toBeUndefined();
  });

  it('rejects stale plan revisions and changed content without mutating live authority', () => {
    const { ctx, artifact } = setup();
    writeRehydrationLedger(artifact, {
      capturedAt: new Date().toISOString(),
      segments: [{ version: 1, id: 'active-plan', kind: 'plan', origin: 'plan-domain', authority: 'user', digest: contentDigest('old'), scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 20 }],
      segmentContents: { 'active-plan': 'old' },
      plan: { scope: 'old-scope', branchSnapshotId: 'old', generation: 1, revision: 'r1' },
      pendingInteractionIds: [], consumerCursors: {},
    });
    const receipt = rehydrateSession(ctx as never, 'tree', {
      getLivePlan: () => ({ scope: 'new-scope', branchSnapshotId: 'new', generation: 3, revision: 'r3', phase: 'accepted', content: 'new' }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    expect(receipt.stale).toContain('active-plan');
    expect(receipt.planDecision).toBe('live-newer');
    expect(receipt.restored).not.toContain('active-plan');
  });

  it('rejects changed active-plan content even when snapshot metadata was reused', () => {
    const { ctx, artifact } = setup();
    writeRehydrationLedger(artifact, {
      capturedAt: new Date().toISOString(),
      segments: [{ version: 1, id: 'active-plan', kind: 'plan', origin: 'plan-domain', authority: 'user', digest: contentDigest('old bytes'), scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 20 }],
      segmentContents: { 'active-plan': 'old bytes' },
      plan: { scope: 'same', branchSnapshotId: 'same', generation: 2, revision: 'same' },
      pendingInteractionIds: [], consumerCursors: {},
    });
    const receipt = rehydrateSession(ctx as never, 'compaction', {
      getLivePlan: () => ({ scope: 'same', branchSnapshotId: 'same', generation: 2, revision: 'same', phase: 'executing', content: 'new bytes' }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    expect(receipt.planDecision).toBe('match');
    const projection = consumeValidatedRehydration(ctx as never, [{
      segment: writeSegment('active-plan', 'plan', 'plan-domain', 'user', 'task', 'transcript', 'new bytes'),
      content: 'new bytes',
    }], { allowProjection: true });
    expect(projection?.receipt.stale).toEqual(['active-plan']);
    expect(projection?.receipt.reprojected).toEqual([]);
    expect(projection?.content).not.toContain('old bytes');
  });

  it.each(['corrupt', 'identity-mismatch'] as const)('fails closed for a %s ledger and leaves durable files untouched', (kind) => {
    const { ctx, artifact } = setup();
    writeRehydrationLedger(artifact, { capturedAt: new Date(0).toISOString(), segments: [], pendingInteractionIds: [], consumerCursors: {} });
    const file = artifact.resolve('compaction/rehydration-v1.json');
    const before = fs.readFileSync(file, 'utf8');
    fs.writeFileSync(file, kind === 'corrupt' ? '{bad' : before.replace('session-1', 'other-session'));
    const receipt = rehydrateSession(ctx as never, 'resume', {
      getLivePlan: () => ({ scope: 'live', branchSnapshotId: 's', generation: 1, phase: 'accepted', content: 'live' }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    expect(receipt.outcome).toBe(kind);
    expect(fs.existsSync(file)).toBe(true);
  });

  it('fork never consumes a parent ledger and requires fresh Start activity', () => {
    const { ctx } = setup('fork-child');
    const setActivity = vi.fn();
    const receipt = rehydrateSession(ctx as never, 'fork', {
      getLivePlan: () => ({ scope: 'fork-scope', branchSnapshotId: 'fork-snapshot', generation: 5, revision: 'r5', phase: 'accepted', content: 'reviewable plan' }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity,
    });
    expect(receipt.outcome).toBe('fork-reset');
    expect(receipt.restored).toEqual([]);
    expect(setActivity).toHaveBeenCalledWith(expect.objectContaining({ kind: 'awaiting_start' }));
  });

  it('does not stage an expired checkpoint for projection', () => {
    const { ctx, artifact } = setup();
    const content = 'expired current bytes';
    writeRehydrationLedger(artifact, {
      capturedAt: '2026-08-26T00:00:00.000Z',
      expiresAt: '2026-08-26T00:01:00.000Z',
      segments: [writeSegment('memory', 'memory-lead', 'memory', 'external-data', 'task', 'inspectable', content)],
      segmentContents: { memory: content }, pendingInteractionIds: [], consumerCursors: {},
    });
    const receipt = rehydrateSession(ctx as never, 'resume', {
      getLivePlan: () => undefined,
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
      now: () => Date.parse('2026-08-26T00:02:00.000Z'),
    });
    expect(receipt.outcome).toBe('expired');
    expect(consumeValidatedRehydration(ctx as never, [{ segment: writeSegment('memory', 'memory-lead', 'memory', 'external-data', 'task', 'inspectable', content), content }], { allowProjection: true })).toBeUndefined();
  });

  it('is idempotent across repeated post-compaction and restart restoration', () => {
    const { ctx, artifact } = setup();
    const content = 'stable';
    writeRehydrationLedger(artifact, {
      capturedAt: new Date().toISOString(),
      segments: [{ version: 1, id: 'policy', kind: 'product-policy', origin: 'octocode-harness', authority: 'product', digest: contentDigest(content), scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20 }],
      segmentContents: { policy: content }, pendingInteractionIds: [], consumerCursors: {},
    });
    const deps = {
      getLivePlan: () => undefined,
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    };
    expect(rehydrateSession(ctx as never, 'compaction', deps).present).toEqual(['policy']);
    expect(consumeValidatedRehydration(ctx as never, [{ segment: writeSegment('policy', 'product-policy', 'octocode-harness', 'product', 'session', 'hidden-policy', content), content }], { allowProjection: true })?.receipt.validated).toEqual(['policy']);
    expect(rehydrateSession(ctx as never, 'resume', deps).present).toEqual(['policy']);
  });

  it('stores a body-safe receipt without checkpoint content', () => {
    const { ctx, artifact } = setup();
    const secretBody = 'segment body must not enter receipts';
    writeRehydrationLedger(artifact, {
      capturedAt: new Date(0).toISOString(),
      segments: [{ version: 1, id: 'policy', kind: 'product-policy', origin: 'octocode-harness', authority: 'product', digest: contentDigest(secretBody), scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20 }],
      segmentContents: { policy: secretBody }, pendingInteractionIds: [], consumerCursors: {},
    });
    const appended: unknown[] = [];
    const receipt = runAndRecordRehydration({ appendEntry: (type: string, data?: unknown) => appended.push({ type, data }) } as never, ctx as never, 'resume', {
      getLivePlan: () => undefined,
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    expect(receipt.present).toEqual(['policy']);
    expect(appended).toEqual([{ type: REHYDRATION_RECEIPT_ENTRY_TYPE, data: receipt }]);
    expect(JSON.stringify(receipt)).not.toContain(secretBody);
  });

  it('never restores artifact-only or changed tool and skill bytes, and enforces projection budget', () => {
    const { ctx, artifact } = setup();
    const oldTool = 'old tool';
    const oldSkill = 'old skill';
    const plan = '<active_plan>bounded live plan</active_plan>';
    writeRehydrationLedger(artifact, {
      capturedAt: new Date().toISOString(),
      segments: [
        writeSegment('tool', 'tool-contract', 'octocode-harness', 'product', 'session', 'inspectable', oldTool),
        writeSegment('skill', 'skill', 'installed-skills', 'project', 'session', 'inspectable', oldSkill, 'on-trigger'),
        writeSegment('orphan', 'memory-lead', 'memory', 'external-data', 'task', 'inspectable', 'artifact only'),
        writeSegment('active-plan', 'plan', 'plan-domain', 'user', 'task', 'transcript', plan),
      ],
      segmentContents: { tool: oldTool, skill: oldSkill, orphan: 'artifact only', 'active-plan': plan },
      pendingInteractionIds: [], consumerCursors: {},
    });
    rehydrateSession(ctx as never, 'compaction', {
      getLivePlan: () => ({ scope: 'none', branchSnapshotId: '', generation: 0, phase: 'executing', content: plan }),
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    const projection = consumeValidatedRehydration(ctx as never, [
      { segment: writeSegment('tool', 'tool-contract', 'octocode-harness', 'product', 'session', 'inspectable', 'new tool'), content: 'new tool' },
      { segment: writeSegment('skill', 'skill', 'installed-skills', 'project', 'session', 'inspectable', 'new skill', 'on-trigger'), content: 'new skill' },
      { segment: writeSegment('orphan', 'memory-lead', 'memory', 'external-data', 'task', 'inspectable', 'artifact only'), content: 'artifact only' },
      { segment: writeSegment('active-plan', 'plan', 'plan-domain', 'user', 'task', 'transcript', plan), content: plan },
    ], { allowProjection: true, totalTokenBudget: 1 });
    expect(projection?.receipt.stale).toEqual(expect.arrayContaining(['tool', 'skill', 'active-plan']));
    expect(projection?.receipt.overBudget).toContain('orphan');
    expect(projection?.content).not.toContain(oldTool);
    expect(projection?.content).not.toContain(oldSkill);
    expect(projection?.content).not.toContain('artifact only');
  });

  it('validates but never replays transcript tool results after compaction', () => {
    const { ctx, artifact } = setup();
    const content = 'large historical tool result';
    const segment = writeSegment('tool-result:1', 'tool-result', 'tool-call:1', 'external-data', 'task', 'inspectable', content, 'summary-only');
    writeRehydrationLedger(artifact, {
      capturedAt: new Date().toISOString(),
      segments: [segment],
      segmentContents: { [segment.id]: content },
      pendingInteractionIds: [], consumerCursors: {},
    });
    rehydrateSession(ctx as never, 'compaction', {
      getLivePlan: () => undefined,
      openContinuity: () => ({ listPendingInteractions: () => [], getConsumerCursor: () => 0, close: vi.fn() }),
      setActivity: vi.fn(),
    });
    const projection = consumeValidatedRehydration(ctx as never, [{ segment, content }], { allowProjection: true });
    expect(projection?.receipt.validated).toEqual([segment.id]);
    expect(projection?.receipt.reprojected).toEqual([]);
    expect(projection?.content).toBe('');
  });
});

function writeSegment(
  id: string,
  kind: 'product-policy' | 'skill' | 'plan' | 'tool-contract' | 'memory-lead' | 'tool-result',
  origin: string,
  authority: 'product' | 'user' | 'project' | 'external-data',
  scope: 'session' | 'task',
  visibility: 'hidden-policy' | 'inspectable' | 'transcript',
  content: string,
  rehydrate: 'always' | 'on-trigger' | 'summary-only' = 'always',
) {
  return { version: 1 as const, id, kind, origin, authority, digest: contentDigest(content), scope, visibility, rehydrate, tokenBudget: 100 };
}
