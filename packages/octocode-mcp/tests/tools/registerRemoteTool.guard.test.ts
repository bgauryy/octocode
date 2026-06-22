import { describe, it, expect, vi } from 'vitest';

import { createRemoteToolRegistration } from '../../src/tools/registerRemoteTool.js';

describe('createRemoteToolRegistration registrationGuard (#T4)', () => {
  it('returns null when the guard returns false', async () => {
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
  });
});
