/** Error rows stay lean because the invocation envelope already names the tool. */
import { describe, it, expect } from 'vitest';
import { handleCatchError } from '../../src/tools/utils.js';

describe('handleCatchError — lean row-local errors', () => {
  it('does not repeat toolName when the invocation already identifies the tool', () => {
    const result = handleCatchError(
      new Error('underlying failure'),
      {},
      undefined,
      'ghSearchCode'
    );
    expect(result).not.toHaveProperty('toolName');
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
