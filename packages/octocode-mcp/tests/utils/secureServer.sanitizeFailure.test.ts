import { afterEach, describe, expect, it, vi } from 'vitest';
import type { CallToolResult, McpServer } from '@modelcontextprotocol/server';
import { allowExpectedStderrWarning } from '../warningPolicy.js';

vi.mock('@octocodeai/octocode-tools-core', async importOriginal => {
  const actual =
    await importOriginal<typeof import('@octocodeai/octocode-tools-core')>();

  return {
    ...actual,
    sanitizeCallToolResult: vi.fn(() => {
      throw new Error('sanitizer boom SECRET_SHOULD_NOT_LEAK');
    }),
  };
});

import { withOutputSanitization } from '../../src/utils/secureServer.js';

function setupSecureServer(): {
  proxy: McpServer;
  getWrapped: () => (...args: unknown[]) => Promise<CallToolResult>;
} {
  let wrapped: ((...args: unknown[]) => Promise<CallToolResult>) | undefined;
  const server = {
    registerTool: vi.fn(
      (_name: string, _config: unknown, callback: unknown) => {
        wrapped = callback as typeof wrapped;
        return {} as never;
      }
    ),
  } as unknown as McpServer;

  return {
    proxy: withOutputSanitization(server),
    getWrapped: () => {
      if (!wrapped) throw new Error('tool callback was not registered');
      return wrapped;
    },
  };
}

describe('createSecureServer sanitizer failures', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('fails closed and discards the original tool result', async () => {
    allowExpectedStderrWarning(
      '[octocode-mcp] output sanitization failed for tool "example"'
    );
    const { proxy, getWrapped } = setupSecureServer();
    proxy.registerTool(
      'example',
      {} as never,
      async () =>
        ({
          content: [{ type: 'text', text: 'original body SECRET_OUTPUT' }],
          isError: false,
        }) as never
    );

    const result = await getWrapped()({}, {});
    const serialized = JSON.stringify(result);

    expect(result?.isError).toBe(true);
    expect(serialized).toContain('Output sanitization failed');
    expect(serialized).toContain('discarded');
    expect(serialized).not.toContain('original body');
    expect(serialized).not.toContain('SECRET_OUTPUT');
  });

  it('does not expose sanitizer failure details in-band or on stderr', async () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const { proxy, getWrapped } = setupSecureServer();
    proxy.registerTool(
      'example',
      {} as never,
      async () =>
        ({
          content: [{ type: 'text', text: 'original body' }],
          isError: false,
        }) as never
    );

    const result = await getWrapped()({}, {});
    const output = stderr.mock.calls.flat().join(' ');

    expect(JSON.stringify(result)).not.toContain('SECRET_SHOULD_NOT_LEAK');
    expect(output).not.toContain('SECRET_SHOULD_NOT_LEAK');
    expect(output).not.toContain('sanitizer boom');
  });
});
