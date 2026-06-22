import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('trySafe', () => {
  let errorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    errorSpy.mockRestore();
  });

  it('returns fn result on success', async () => {
    const { trySafe } = await import('../../src/utils/try-safe.js');
    expect(trySafe(() => 7, 0)).toBe(7);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('returns fallback on throw', async () => {
    const { trySafe } = await import('../../src/utils/try-safe.js');
    expect(
      trySafe(() => {
        throw new Error('boom');
      }, 'fallback')
    ).toBe('fallback');
  });

  it('stays silent on throw', async () => {
    const { trySafe } = await import('../../src/utils/try-safe.js');
    trySafe(() => {
      throw new Error('silent');
    }, null);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('handles non-Error throws', async () => {
    const { trySafe } = await import('../../src/utils/try-safe.js');
    expect(
      trySafe(() => {
        throw 'plain';
      }, 'fb')
    ).toBe('fb');
    expect(errorSpy).not.toHaveBeenCalled();
  });
});

describe('trySafeAsync', () => {
  let errorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    errorSpy.mockRestore();
  });

  it('returns resolved value on success', async () => {
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await expect(trySafeAsync(async () => 42, 0)).resolves.toBe(42);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('returns fallback on rejection', async () => {
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await expect(
      trySafeAsync(async () => {
        throw new Error('async boom');
      }, 'fb')
    ).resolves.toBe('fb');
  });

  it('stays silent on rejection', async () => {
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await trySafeAsync(async () => {
      throw new Error('quiet');
    }, undefined);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('handles non-Error rejection', async () => {
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await expect(
      trySafeAsync(async () => {
        throw 'async plain';
      }, null)
    ).resolves.toBeNull();
    expect(errorSpy).not.toHaveBeenCalled();
  });
});
