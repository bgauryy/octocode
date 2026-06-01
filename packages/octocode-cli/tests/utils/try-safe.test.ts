import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('trySafe', () => {
  let errorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    delete process.env.OCTOCODE_DEBUG;
    errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    errorSpy.mockRestore();
    delete process.env.OCTOCODE_DEBUG;
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

  it('does not log when OCTOCODE_DEBUG is not 1', async () => {
    process.env.OCTOCODE_DEBUG = '0';
    vi.resetModules();
    const { trySafe } = await import('../../src/utils/try-safe.js');
    trySafe(() => {
      throw new Error('silent');
    }, null);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('logs when OCTOCODE_DEBUG is 1', async () => {
    process.env.OCTOCODE_DEBUG = '1';
    vi.resetModules();
    const { trySafe } = await import('../../src/utils/try-safe.js');
    trySafe(() => {
      throw new Error('loud');
    }, null);
    expect(errorSpy).toHaveBeenCalledWith('[trySafe]', 'loud');
  });

  it('handles non-Error throws in debug mode', async () => {
    process.env.OCTOCODE_DEBUG = '1';
    vi.resetModules();
    const { trySafe } = await import('../../src/utils/try-safe.js');
    trySafe(() => {
      throw 'plain';
    }, 'fb');
    expect(errorSpy).toHaveBeenCalledWith('[trySafe]', 'plain');
  });
});

describe('trySafeAsync', () => {
  let errorSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    delete process.env.OCTOCODE_DEBUG;
    errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    errorSpy.mockRestore();
    delete process.env.OCTOCODE_DEBUG;
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

  it('does not log async errors when debug off', async () => {
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await trySafeAsync(async () => {
      throw new Error('quiet');
    }, undefined);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('logs async errors when OCTOCODE_DEBUG is 1', async () => {
    process.env.OCTOCODE_DEBUG = '1';
    vi.resetModules();
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await trySafeAsync(async () => {
      throw new Error('async loud');
    }, undefined);
    expect(errorSpy).toHaveBeenCalledWith('[trySafeAsync]', 'async loud');
  });

  it('handles non-Error rejection in debug mode', async () => {
    process.env.OCTOCODE_DEBUG = '1';
    vi.resetModules();
    const { trySafeAsync } = await import('../../src/utils/try-safe.js');
    await trySafeAsync(async () => {
      throw 'async plain';
    }, null);
    expect(errorSpy).toHaveBeenCalledWith('[trySafeAsync]', 'async plain');
  });
});
