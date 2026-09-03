import { describe, expect, it } from 'vitest';
import { contentDigest } from '@octocodeai/octocode-awareness';
import { PROMPT_LIFECYCLE_MATRIX, canReprojectRehydratedSegment, promptLifecycleFor } from '../src/tools/prompt-lifecycle.js';

describe('prompt lifecycle matrix', () => {
  it('classifies every context kind exactly once', () => {
    expect(Object.keys(PROMPT_LIFECYCLE_MATRIX).sort()).toEqual([
      'memory-lead', 'peer-event', 'plan', 'product-policy', 'project-instruction', 'skill', 'tool-contract', 'tool-result', 'user-request',
    ]);
  });

  it('keeps cacheable contracts frozen and mutable state out of frozen bytes', () => {
    for (const kind of ['product-policy', 'project-instruction', 'tool-contract', 'skill'] as const) {
      expect(promptLifecycleFor(kind).placement).toBe('frozen-system');
      expect(promptLifecycleFor(kind).mutable).toBe(false);
    }
    for (const kind of ['plan', 'peer-event', 'memory-lead'] as const) {
      expect(promptLifecycleFor(kind).placement).not.toBe('frozen-system');
    }
  });

  it('reloads selected attributable state without promoting frozen catalogs', () => {
    const segment = (kind: 'user-request' | 'tool-result' | 'peer-event' | 'memory-lead' | 'skill', authority: 'user' | 'external-data' | 'project') => ({
      version: 1 as const,
      id: kind,
      kind,
      origin: `owner:${kind}`,
      authority,
      digest: contentDigest(kind),
      scope: 'task' as const,
      visibility: 'inspectable' as const,
      rehydrate: 'always' as const,
    });
    expect(canReprojectRehydratedSegment(segment('user-request', 'user'))).toBe(true);
    expect(canReprojectRehydratedSegment(segment('tool-result', 'external-data'))).toBe(true);
    expect(canReprojectRehydratedSegment(segment('peer-event', 'external-data'))).toBe(true);
    expect(canReprojectRehydratedSegment(segment('memory-lead', 'external-data'))).toBe(true);
    expect(canReprojectRehydratedSegment(segment('skill', 'project'))).toBe(true);
    expect(canReprojectRehydratedSegment({ ...segment('skill', 'project'), rehydrate: 'on-trigger' })).toBe(false);
    expect(canReprojectRehydratedSegment({ ...segment('user-request', 'user'), visibility: 'hidden-policy' })).toBe(false);
    expect(promptLifecycleFor('tool-contract').restoration).toBe('reload-owner');
    expect(canReprojectRehydratedSegment({
      ...segment('tool-result', 'external-data'),
      kind: 'tool-contract',
      origin: 'mcp:demand-loaded',
    })).toBe(true);
    expect(canReprojectRehydratedSegment({
      ...segment('tool-result', 'external-data'),
      kind: 'tool-contract',
      origin: 'octocode-harness',
      authority: 'product',
      scope: 'session',
    })).toBe(false);
  });
});
