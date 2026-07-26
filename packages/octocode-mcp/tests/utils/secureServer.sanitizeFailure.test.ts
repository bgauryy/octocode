import { describe, it, expect, vi } from 'vitest';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

// Force output sanitization to throw so we exercise the failure branch in
// wrapToolCallback. Only `sanitizeCallToolResult` is overridden; the rest of the
// module (ContentSanitizer/maskSensitiveData used to scrub the warning reason,
// buildToolErrorResult) stays real. Isolated to its own file so the suite's
// real-sanitizer tests are unaffected by the module mock.
vi.mock('@octocodeai/octocode-tools-core', async importActual => {
  const actual =
    await importActual<typeof import('@octocodeai/octocode-tools-core')>();
  return {
    ...actual,
    sanitizeCallToolResult: () => {
      throw new Error(
        'sanitizer boom ghp_abc123xyz456789012345678901234567890'
      );
    },
  };
});

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { withOutputSanitization } from '../../src/utils/secureServer.js';

function makeProxy() {
  let capturedCb!: (...args: unknown[]) => Promise<CallToolResult>;
  const server = {
    registerTool: vi.fn((_name: string, _config: unknown, cb: unknown) => {
      capturedCb = cb as typeof capturedCb;
      return {} as never;
    }),
  } as unknown as McpServer;
  const proxy = withOutputSanitization(server);
  return { proxy, getCb: () => capturedCb };
}

describe('secureServer — sanitization-failure policy', () => {
  it('warns and returns the result instead of hard-failing or silently passing it raw', async () => {
    const { proxy, getCb } = makeProxy();
    const raw: CallToolResult = {
      content: [{ type: 'text', text: 'original body' }],
      isError: false,
    };
    proxy.registerTool(
      't',
      {} as never,
      vi.fn().mockResolvedValue(raw) as never
    );

    const result = await getCb()({});

    // 1. Not a hard failure — the tool result is still delivered.
    expect(result.isError).toBe(false);
    // 2. Not a silent raw passthrough — a visible warning is prepended.
    const first = result.content[0] as { type: 'text'; text: string };
    expect(first.text).toContain('sanitization failed');
    // 3. The original content is preserved (after the warning).
    expect(
      result.content.some(
        c => (c as { text?: string }).text === 'original body'
      )
    ).toBe(true);
  });

  it('scrubs the failure reason so the warning cannot leak a secret', async () => {
    const { proxy, getCb } = makeProxy();
    proxy.registerTool(
      't',
      {} as never,
      vi
        .fn()
        .mockResolvedValue({ content: [] } satisfies CallToolResult) as never
    );

    const result = await getCb()({});
    const warning = result.content[0] as { type: 'text'; text: string };
    expect(warning.text).toContain('sanitization failed');
    expect(warning.text).not.toContain(
      'ghp_abc123xyz456789012345678901234567890'
    );
  });
});
