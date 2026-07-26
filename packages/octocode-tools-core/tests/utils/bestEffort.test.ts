import { describe, expect, it } from 'vitest';
import {
  ignoreBestEffortFailure,
  fallbackOnBestEffortFailure,
} from '../../src/utils/core/bestEffort.js';

describe('ignoreBestEffortFailure', () => {
  it('returns a function', () => {
    expect(typeof ignoreBestEffortFailure('test reason')).toBe('function');
  });

  it('returned handler does not throw when called with any error', () => {
    const handler = ignoreBestEffortFailure('network timeout');
    expect(() => handler(new Error('some error'))).not.toThrow();
    expect(() => handler(null)).not.toThrow();
    expect(() => handler(undefined)).not.toThrow();
    expect(() => handler({ code: 404 })).not.toThrow();
  });

  it('returned handler returns undefined (void)', () => {
    const handler = ignoreBestEffortFailure('reason');
    expect(handler(new Error('oops'))).toBeUndefined();
  });

  it('can be used as a .catch() handler in a promise chain', async () => {
    const result = await Promise.reject(new Error('fail'))
      .then(() => 'success')
      .catch(ignoreBestEffortFailure('safe to ignore'));
    // The handler returns void so the chain resolves to undefined
    expect(result).toBeUndefined();
  });
});

describe('fallbackOnBestEffortFailure', () => {
  it('returns a function', () => {
    expect(typeof fallbackOnBestEffortFailure('reason', 'default')).toBe(
      'function'
    );
  });

  it('returned handler returns the fallback value', () => {
    const handler = fallbackOnBestEffortFailure('reason', 42);
    expect(handler(new Error('oops'))).toBe(42);
  });

  it('handles null fallback', () => {
    const handler = fallbackOnBestEffortFailure('reason', null);
    expect(handler(new Error('oops'))).toBeNull();
  });

  it('handles array fallback', () => {
    const handler = fallbackOnBestEffortFailure('reason', [] as string[]);
    expect(handler(new Error('oops'))).toEqual([]);
  });

  it('can be used as a .catch() handler that provides a default value', async () => {
    const result = await Promise.reject(new Error('fail'))
      .then(() => 'success')
      .catch(fallbackOnBestEffortFailure('safe', 'default-value'));
    expect(result).toBe('default-value');
  });

  it('works with object fallback', () => {
    const fallback = { count: 0, items: [] as string[] };
    const handler = fallbackOnBestEffortFailure('reason', fallback);
    expect(handler(new Error('fail'))).toEqual({ count: 0, items: [] });
  });
});
