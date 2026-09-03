import { contentDigest } from '@octocodeai/octocode-awareness';
import { afterEach, describe, expect, it } from 'vitest';
import type { PiContext } from '../src/types.js';
import { mergeCompactionRehydrationCaptures } from '../src/tools/compaction-hooks.js';
import {
  captureCurrentContextSources,
  clearCurrentContextSources,
  mergeCurrentContextSources,
  readLatestSessionUserRequest,
  readSessionPeerEvent,
  readSessionToolResult,
  registerCurrentContextSource,
  resolveSessionCheckpointSources,
  sessionPeerEventOrigin,
  sessionToolResultOrigin,
  sessionUserRequestOrigin,
} from '../src/tools/context-source-registry.js';

const ctx = {
  cwd: '/workspace',
  sessionManager: { getSessionId: () => 'context-registry-session' },
} as PiContext;

afterEach(() => clearCurrentContextSources());

describe('production current-context source registry', () => {
  it('captures every dynamic owner with current digests, budgets, and non-product authority', () => {
    const values: Record<string, string> = {
      request: 'fix the recovery flow',
      peer: 'peer evidence',
      tool: 'tool evidence',
      memory: 'memory lead',
      skill: 'selected skill bytes',
      demand: 'demand-loaded tool state',
    };
    const owners = [
      { id: 'request', kind: 'user-request', origin: 'session-entry:u1', authority: 'user', scope: 'task', visibility: 'transcript' },
      { id: 'peer', kind: 'peer-event', origin: 'awareness-event:e1', authority: 'external-data', scope: 'turn', visibility: 'inspectable' },
      { id: 'tool', kind: 'tool-result', origin: 'session-entry:t1', authority: 'external-data', scope: 'turn', visibility: 'transcript' },
      { id: 'memory', kind: 'memory-lead', origin: 'memory:m1', authority: 'external-data', scope: 'task', visibility: 'inspectable' },
      { id: 'skill', kind: 'skill', origin: 'skill-file:review', authority: 'project', scope: 'task', visibility: 'inspectable' },
      { id: 'demand', kind: 'tool-contract', origin: 'mcp:server/tool', authority: 'external-data', scope: 'task', visibility: 'inspectable' },
    ] as const;
    for (const owner of owners) {
      registerCurrentContextSource(ctx, {
        version: 1,
        ...owner,
        rehydrate: 'always',
        readCurrent: () => values[owner.id],
      });
    }

    const first = captureCurrentContextSources(ctx);
    expect(first.segments.map((segment) => segment.id)).toEqual(owners.map((owner) => owner.id));
    expect(first.segments.map((segment) => segment.authority)).not.toContain('product');
    expect(first.segments.every((segment) => segment.tokenBudget !== undefined)).toBe(true);
    expect(first.contents).toEqual(values);

    values.peer = 'new peer evidence';
    const second = captureCurrentContextSources(ctx);
    expect(second.segments.find((segment) => segment.id === 'peer')?.digest).toBe(contentDigest('new peer evidence'));
    expect(second.segments.find((segment) => segment.id === 'peer')?.digest).not.toBe(first.segments.find((segment) => segment.id === 'peer')?.digest);
  });

  it('fails registration closed on authority promotion', () => {
    const common = { version: 1 as const, scope: 'task' as const, visibility: 'inspectable' as const, rehydrate: 'always' as const, readCurrent: () => 'x' };
    expect(() => registerCurrentContextSource(ctx, { ...common, id: 'peer', kind: 'peer-event', origin: 'peer', authority: 'product' })).toThrow(/external-data/);
    expect(() => registerCurrentContextSource(ctx, { ...common, id: 'skill', kind: 'skill', origin: 'skill', authority: 'product' })).toThrow(/cannot select product/);
    expect(() => registerCurrentContextSource(ctx, { ...common, id: 'tool', kind: 'tool-contract', origin: 'mcp', authority: 'product' })).toThrow(/cannot select product/);
  });

  it('skips unavailable and over-budget owners without persisting their bytes', () => {
    registerCurrentContextSource(ctx, {
      version: 1, id: 'missing', kind: 'memory-lead', origin: 'memory:missing', authority: 'external-data',
      scope: 'task', visibility: 'inspectable', rehydrate: 'always', readCurrent: () => undefined,
    });
    registerCurrentContextSource(ctx, {
      version: 1, id: 'large', kind: 'tool-result', origin: 'tool:large', authority: 'external-data',
      scope: 'turn', visibility: 'transcript', rehydrate: 'always', tokenBudget: 1, readCurrent: () => '12345678',
    });
    const capture = captureCurrentContextSources(ctx);
    expect(capture).toMatchObject({ unavailable: ['missing'], overBudget: ['large'], segments: [], contents: {} });
  });

  it('keeps fixed identities authoritative when merging capture and restore sources', () => {
    registerCurrentContextSource(ctx, {
      version: 1, id: 'fixed', kind: 'tool-result', origin: 'tool:dynamic', authority: 'external-data',
      scope: 'turn', visibility: 'transcript', rehydrate: 'always', readCurrent: () => 'dynamic bytes',
    });
    const fixedSegment = {
      version: 1 as const, id: 'fixed', kind: 'tool-contract' as const, origin: 'octocode-harness', authority: 'product' as const,
      digest: contentDigest('fixed bytes'), scope: 'session' as const, visibility: 'inspectable' as const, rehydrate: 'always' as const,
    };
    expect(mergeCurrentContextSources(ctx, [{ segment: fixedSegment, content: 'fixed bytes' }])).toEqual([
      { segment: fixedSegment, content: 'fixed bytes' },
    ]);
    expect(mergeCompactionRehydrationCaptures(
      { segments: [fixedSegment], contents: { fixed: 'fixed bytes' } },
      captureCurrentContextSources(ctx),
    )).toEqual({ segments: [fixedSegment], contents: { fixed: 'fixed bytes' } });
  });

  it('keeps restore-only skill/tool owners out of capture but available to current validation', () => {
    registerCurrentContextSource(ctx, {
      version: 1, id: 'selected-skill:review', kind: 'skill', origin: 'skill-file:review', authority: 'project',
      scope: 'task', visibility: 'inspectable', rehydrate: 'always', capture: false, readCurrent: () => 'current skill bytes',
    });
    expect(captureCurrentContextSources(ctx).segments).toEqual([]);
    expect(mergeCurrentContextSources(ctx, []).map((source) => source.segment.id)).toEqual(['selected-skill:review']);
  });

  it('rebuilds transcript-owned sources from durable session entries after restart', () => {
    const transcriptCtx = {
      cwd: '/workspace',
      sessionManager: {
        getSessionId: () => 'transcript-session',
        getEntries: () => [
          { type: 'message', message: { role: 'user', content: [{ type: 'text', text: 'original request' }] } },
          { type: 'message', message: { role: 'toolResult', toolCallId: 'call/1', content: [{ type: 'text', text: 'tool bytes' }] } },
          { type: 'message', message: { customType: 'octocode-peer-event', content: 'peer bytes', details: { eventId: 'event/1' } } },
        ],
      },
    } as PiContext;
    expect(readLatestSessionUserRequest(transcriptCtx)).toBe('original request');
    expect(readSessionToolResult(transcriptCtx, 'call/1')).toBe('tool bytes');
    expect(readSessionPeerEvent(transcriptCtx, 'event/1')).toBe('peer bytes');

    const segment = (id: string, kind: 'user-request' | 'tool-result' | 'peer-event', origin: string, content: string) => ({
      version: 1 as const,
      id,
      kind,
      origin,
      authority: (kind === 'user-request' ? 'user' : 'external-data') as 'user' | 'external-data',
      digest: contentDigest(content),
      scope: 'turn' as const,
      visibility: 'transcript' as const,
      rehydrate: 'always' as const,
      tokenBudget: 100,
    });
    const sources = resolveSessionCheckpointSources(transcriptCtx, [
      segment('request', 'user-request', sessionUserRequestOrigin(), 'original request'),
      segment('tool', 'tool-result', sessionToolResultOrigin('call/1'), 'tool bytes'),
      segment('peer', 'peer-event', sessionPeerEventOrigin('event/1'), 'peer bytes'),
    ]);
    expect(sources.map((source) => [source.segment.id, source.content])).toEqual([
      ['request', 'original request'], ['tool', 'tool bytes'], ['peer', 'peer bytes'],
    ]);
  });
});
