import { describe, expect, it, vi } from 'vitest';
import { invokeCallbackSafely } from '../../src/tools/utils.js';

describe('invokeCallbackSafely', () => {
  it('does nothing when callback is undefined', async () => {
    await expect(
      invokeCallbackSafely(undefined, 'localSearch', [{}])
    ).resolves.toBeUndefined();
  });

  it('invokes callback with toolName and queries', async () => {
    const cb = vi.fn().mockResolvedValue(undefined);
    await invokeCallbackSafely(cb, 'localSearch', [{ path: '/src' }]);
    expect(cb).toHaveBeenCalledWith('localSearch', [{ path: '/src' }]);
  });

  it('swallows errors thrown by callback', async () => {
    const cb = vi.fn().mockRejectedValue(new Error('cb failed'));
    await expect(
      invokeCallbackSafely(cb, 'localSearch', [{}])
    ).resolves.toBeUndefined();
  });
});
