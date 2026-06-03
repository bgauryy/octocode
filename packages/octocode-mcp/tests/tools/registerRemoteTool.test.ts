import { describe, it, expect, vi } from 'vitest';
import { z } from 'zod';
import { createRemoteToolRegistration } from '../../src/tools/registerRemoteTool.js';

function createMockServer() {
  const registered: Array<{ name: string; handler: unknown }> = [];
  const server = {
    registerTool: vi.fn((name: string, _opts: unknown, handler: unknown) => {
      registered.push({ name, handler });
      return { name, registered: true };
    }),
  };
  return { server, registered };
}

const trivialInput = z.object({ queries: z.array(z.object({})).optional() });
const trivialOutput = z.object({});

describe('createRemoteToolRegistration — registrationGuard', () => {
  it('registers the tool synchronously when no guard is provided', () => {
    const { server } = createMockServer();
    const register = createRemoteToolRegistration({
      name: 'sync_tool',
      title: 'Sync',
      inputSchema: trivialInput,
      outputSchema: trivialOutput,
      executionFn: async () => ({
        content: [{ type: 'text' as const, text: 'ok' }],
      }),
    });
    const result = register(server as never);
    expect(result).not.toBeNull();
    expect(result).not.toBeInstanceOf(Promise);
    expect(server.registerTool).toHaveBeenCalledTimes(1);
  });

  it('does NOT register the tool when guard resolves to false', async () => {
    const { server } = createMockServer();
    const register = createRemoteToolRegistration({
      name: 'guarded_tool',
      title: 'Guarded',
      inputSchema: trivialInput,
      outputSchema: trivialOutput,
      registrationGuard: async () => false,
      executionFn: async () => ({
        content: [{ type: 'text' as const, text: 'ok' }],
      }),
    });
    const result = await register(server as never);
    expect(result).toBeNull();
    expect(server.registerTool).not.toHaveBeenCalled();
  });

  it('registers the tool when guard resolves to true', async () => {
    const { server } = createMockServer();
    const register = createRemoteToolRegistration({
      name: 'guarded_tool',
      title: 'Guarded',
      inputSchema: trivialInput,
      outputSchema: trivialOutput,
      registrationGuard: async () => true,
      executionFn: async () => ({
        content: [{ type: 'text' as const, text: 'ok' }],
      }),
    });
    const result = await register(server as never);
    expect(result).not.toBeNull();
    expect(server.registerTool).toHaveBeenCalledTimes(1);
  });
});
