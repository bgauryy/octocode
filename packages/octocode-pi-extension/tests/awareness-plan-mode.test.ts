import { describe, expect, it } from 'vitest';
import { ROUTINE_AWARENESS_OPERATIONS } from '@octocodeai/octocode-awareness';
import { evaluateToolCapability, getToolEffect } from '../src/tools/plan-mode.js';

describe('Awareness discovery capability classification', () => {
  it.each(ROUTINE_AWARENESS_OPERATIONS)('classifies %s schema discovery without execution parameters', operation => {
    const input = { queries: [{ operation, describe: true }] };
    expect(getToolEffect('awareness', input)).toBe('read');
    const receipt = evaluateToolCapability({ toolName: 'awareness', toolInput: input, phase: 'in_review' });
    expect(receipt.resource).toBe('read');
    expect(receipt.effectiveDecision).toBe('allow');
  });

  it('retains the strongest execution effect when discovery shares a batch', () => {
    expect(getToolEffect('awareness', { queries: [
      { operation: 'message.send', describe: true },
      { operation: 'history.restore', params: { action: 'apply', preview_id: 'preview-1' } },
    ] })).toBe('workspace-write');
    expect(getToolEffect('awareness', { queries: [
      { operation: 'history.restore', describe: true },
      { operation: 'message.send', describe: false, params: { kind: 'fyi', subject: 'status' } },
    ] })).toBe('coordination-write');
  });

  it.each([
    { operation: 'history.restore', describe: true, params: { action: 'apply', preview_id: 'preview-1' } },
    { operation: 'message.send', describe: true, params: {} },
    { operation: 'unknown.operation', describe: true },
  ])('does not classify invalid discovery as a harmless read: %j', query => {
    const input = { queries: [query] };
    expect(getToolEffect('awareness', input)).toBeUndefined();
    expect(evaluateToolCapability({ toolName: 'awareness', toolInput: input }).effectiveDecision).toBe('block');
  });

  it('does not throw and treats invalid execution params as unclassified (regression: stale cwd/include_messages/include_work)', () => {
    // Stale params that were removed from the context.orient strict schema.
    // descriptor.effect() previously threw, propagating through the hook error handler.
    const input = { queries: [{ operation: 'context.orient', params: { cwd: '/some/path', include_messages: true, include_work: true } }] };
    expect(() => getToolEffect('awareness', input)).not.toThrow();
    expect(getToolEffect('awareness', input)).toBeUndefined();
    expect(evaluateToolCapability({ toolName: 'awareness', toolInput: input }).effectiveDecision).toBe('block');
  });
});
