import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { contentDigest } from '@octocodeai/octocode-awareness';
import { createSessionArtifactContext, inspectRehydrationLedger, readRehydrationLedger, readRehydrationSegmentContents, resolveRehydrationSegments, writeRehydrationLedger } from '../src/tools/session-artifacts.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => fs.rmSync(root, { recursive: true, force: true })));

describe('compaction rehydration ledger', () => {
  it('is idempotent for the same checkpoint and rejects digest drift', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-'));
    roots.push(workspace);
    const ctx = createSessionArtifactContext({ cwd: workspace, processId: 123, sessionManager: { getSessionId: () => 'session-1' } });
    const segment = { version: 1, id: 'plan', kind: 'plan', origin: 'plan-domain', authority: 'user', digest: contentDigest('plan-v1'), scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 10 } as const;
    const input = { capturedAt: '2026-08-26T00:00:00.000Z', segments: [segment], pendingInteractionIds: ['i1', 'i1'], consumerCursors: { ui: 2 } };
    expect(writeRehydrationLedger(ctx, input).digest).toBe(writeRehydrationLedger(ctx, input).digest);
    const ledger = readRehydrationLedger(ctx)!;
    expect(ledger.pendingInteractionIds).toEqual(['i1']);
    expect(resolveRehydrationSegments(ledger, { plan: 'plan-v1' })).toMatchObject({ restored: ['plan'], stale: [], estimatedTokens: 2 });
    expect(resolveRehydrationSegments(ledger, { plan: 'plan-v2' })).toMatchObject({ restored: [], stale: ['plan'] });
  });

  it('does not restore segments above their declared budget', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-budget-'));
    roots.push(workspace);
    const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'session-2' } });
    const content = '12345678';
    const ledger = writeRehydrationLedger(ctx, { capturedAt: new Date(0).toISOString(), segments: [{ version: 1, id: 'small', kind: 'plan', origin: 'plan', authority: 'user', digest: contentDigest(content), scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 1 }], pendingInteractionIds: [], consumerCursors: {} });
    expect(resolveRehydrationSegments(ledger, { small: content }).overBudget).toEqual(['small']);
  });

  it('persists only digest-bound contained content references and enforces an aggregate budget', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-refs-'));
    roots.push(workspace);
    const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'session-refs' } });
    const one = '12345678';
    const two = 'abcdefgh';
    const ledger = writeRehydrationLedger(ctx, {
      capturedAt: new Date(0).toISOString(),
      segments: [one, two].map((content, index) => ({
        version: 1 as const,
        id: `segment-${index + 1}`,
        kind: 'tool-contract' as const,
        origin: 'octocode-harness',
        authority: 'product' as const,
        digest: contentDigest(content),
        scope: 'session' as const,
        visibility: 'inspectable' as const,
        rehydrate: 'always' as const,
        tokenBudget: 4,
      })),
      segmentContents: { 'segment-1': one, 'segment-2': two },
      pendingInteractionIds: [],
      consumerCursors: {},
    });
    expect(ledger.workspace).toBe(ctx.identity.workspace);
    expect(Object.keys(ledger.contentRefs ?? {})).toEqual(['segment-1', 'segment-2']);
    const contents = readRehydrationSegmentContents(ctx, ledger);
    expect(contents).toEqual({ 'segment-1': one, 'segment-2': two });
    expect(resolveRehydrationSegments(ledger, contents, { totalTokenBudget: 3 })).toMatchObject({
      restored: ['segment-1'],
      overBudget: ['segment-2'],
      estimatedTokens: 2,
    });
  });

  it('classifies corrupt and identity-mismatched ledgers without deleting artifacts', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-corrupt-'));
    roots.push(workspace);
    const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'session-corrupt' } });
    writeRehydrationLedger(ctx, { capturedAt: new Date(0).toISOString(), segments: [], pendingInteractionIds: [], consumerCursors: {} });
    const ledgerPath = ctx.resolve('compaction/rehydration-v1.json');
    const original = fs.readFileSync(ledgerPath, 'utf8');
    fs.writeFileSync(ledgerPath, original.replace('session-corrupt', 'other-session'));
    expect(inspectRehydrationLedger(ctx).status).toBe('identity-mismatch');
    expect(fs.existsSync(ledgerPath)).toBe(true);
    fs.writeFileSync(ledgerPath, '{broken');
    expect(inspectRehydrationLedger(ctx).status).toBe('corrupt');
    expect(fs.existsSync(ledgerPath)).toBe(true);
  });

  it('rejects authority escalation, duplicate IDs, invalid digests, and invalid budgets at the ledger boundary', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'rehydration-segment-validation-'));
    roots.push(workspace);
    const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'session-validation' } });
    const base = {
      version: 1 as const,
      id: 'peer',
      kind: 'peer-event' as const,
      origin: 'peer:one',
      authority: 'external-data' as const,
      digest: contentDigest('peer bytes'),
      scope: 'turn' as const,
      visibility: 'inspectable' as const,
      rehydrate: 'always' as const,
      tokenBudget: 10,
    };
    const input = { capturedAt: new Date().toISOString(), pendingInteractionIds: [], consumerCursors: {} };
    expect(() => writeRehydrationLedger(ctx, { ...input, segments: [{ ...base, authority: 'product' as const }] })).toThrow(/external-data/);
    expect(() => writeRehydrationLedger(ctx, { ...input, segments: [base, base] })).toThrow(/Duplicate/);
    expect(() => writeRehydrationLedger(ctx, { ...input, segments: [{ ...base, digest: 'sha256:not-a-digest' }] })).toThrow(/Invalid rehydration digest/);
    expect(() => writeRehydrationLedger(ctx, { ...input, segments: [{ ...base, tokenBudget: 0 }] })).toThrow(/token budget/);
  });
});
