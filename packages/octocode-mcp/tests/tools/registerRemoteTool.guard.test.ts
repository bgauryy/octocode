import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockLogSessionError = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock('../../src/session.js', () => ({
  logSessionError: mockLogSessionError,
}));

import { createRemoteToolRegistration } from '../../src/tools/registerRemoteTool.js';

describe('createRemoteToolRegistration registrationGuard (#T4)', () => {
  beforeEach(() => mockLogSessionError.mockClear());

  it('logs a skip and returns null when the guard returns false', async () => {
    const register = createRemoteToolRegistration<{ queries: unknown[] }>({
      name: 'testTool',
      title: 'Test',
      inputSchema: {},
      outputSchema: {} as never,
      executionFn: async () =>
        ({ content: [] }) as unknown as ReturnType<
          () => Promise<{ content: [] }>
        >,
      registrationGuard: async () => false,
    } as never);

    const fakeServer = { registerTool: vi.fn() } as never;
    const result = await register(fakeServer);

    expect(result).toBeNull();
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'testTool',
      expect.stringContaining('registration-skipped')
    );
  });
});
