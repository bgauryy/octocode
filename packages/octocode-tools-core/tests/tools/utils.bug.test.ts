/**
 * TDD proof for Bug #1: `void toolName` in handleCatchError silently discards
 * the toolName parameter.  This file should be RED before the fix, GREEN after.
 */
import { describe, it, expect } from 'vitest';
import { handleCatchError } from '../../src/tools/utils.js';

describe('handleCatchError - Bug #1: toolName silently discarded (void toolName)', () => {
  it('includes toolName in the error result when provided', () => {
    const result = handleCatchError(
      new Error('underlying failure'),
      {},
      undefined,
      'ghSearchCode'
    );
    // BUG: toolName is voided in the implementation, so it never reaches the result.
    // This assertion FAILS before the fix.
    expect(result).toMatchObject({ toolName: 'ghSearchCode' });
  });

  it('does not include toolName key when no toolName is given', () => {
    const result = handleCatchError(new Error('bare error'), {});
    expect(result).not.toHaveProperty('toolName');
  });

  it('still produces a valid error result regardless of toolName', () => {
    const result = handleCatchError(new Error('oops'), {}, 'ctx', 'myTool');
    expect(result.status).toBe('error');
    expect(typeof result.error).toBe('string');
  });

  it('contextMessage is still prepended to the error string', () => {
    const result = handleCatchError(
      new Error('connection refused'),
      {},
      'fetch failed',
      'ghGetFile'
    );
    expect(result.error).toBe('fetch failed: connection refused');
  });
});
