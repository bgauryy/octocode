import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const startNativeMcp = vi.hoisted(() => vi.fn(async () => ({})));

vi.mock('../src/native/index.js', () => ({ startNativeMcp }));

describe('native MCP process entry', () => {
  beforeEach(() => {
    vi.resetModules();
    startNativeMcp.mockReset();
    startNativeMcp.mockResolvedValue({});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('starts the native adapter directly', async () => {
    await import('../src/index.js');
    await vi.waitFor(() => expect(startNativeMcp).toHaveBeenCalledOnce());
  });

  it.each([
    [new Error('native boom'), 'native boom'],
    ['native string failure', 'native string failure'],
    [null, 'unknown'],
  ])(
    'reports native startup failure %p and exits',
    async (failure, message) => {
      startNativeMcp.mockRejectedValueOnce(failure);
      const stderr = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);
      const exit = vi
        .spyOn(process, 'exit')
        .mockImplementation((() => undefined) as never);

      await import('../src/index.js');
      await vi.waitFor(() => expect(exit).toHaveBeenCalledWith(1));
      expect(stderr).toHaveBeenCalledWith(
        `Server initialization failed: ${message}\n`
      );
    }
  );
});
