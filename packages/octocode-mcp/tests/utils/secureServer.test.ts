import { describe, it, expect, vi, beforeEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  buildToolErrorResult,
  sanitizeCallToolResult,
  withOutputSanitization,
} from '../../src/utils/secureServer.js';

describe('secureServer', () => {
  describe('sanitizeCallToolResult', () => {
    it('should redact secrets in content[] text items', () => {
      const result: CallToolResult = {
        content: [
          {
            type: 'text',
            text: 'token: ghp_abc123xyz456789012345678901234567890',
          },
        ],
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.content[0]).toHaveProperty('type', 'text');
      const text = (sanitized.content[0] as { type: 'text'; text: string })
        .text;
      expect(text).not.toContain('ghp_abc123xyz456789012345678901234567890');
      expect(text).toContain('[REDACTED-');
    });

    it('should pass through text items without secrets unchanged', () => {
      const result: CallToolResult = {
        content: [{ type: 'text', text: 'hello world' }],
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.content[0]).toEqual({
        type: 'text',
        text: 'hello world',
      });
    });

    it('should pass through non-text content items unchanged', () => {
      const imageItem = {
        type: 'image' as const,
        data: 'base64data',
        mimeType: 'image/png',
      };
      const result: CallToolResult = { content: [imageItem] };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.content[0]).toBe(imageItem);
    });

    it('should deep-walk and redact secrets in structuredContent', () => {
      const result: CallToolResult = {
        content: [],
        structuredContent: {
          data: {
            nested: 'token: ghp_abc123xyz456789012345678901234567890',
          },
          safe: 'no secrets here',
        },
      };
      const sanitized = sanitizeCallToolResult(result);
      const nested = (sanitized.structuredContent as Record<string, unknown>)
        .data as Record<string, unknown>;
      expect(nested.nested).not.toContain(
        'ghp_abc123xyz456789012345678901234567890'
      );
    });

    it('should sanitize structuredContent arrays', () => {
      const result: CallToolResult = {
        content: [],
        structuredContent: {
          items: ['safe', 'ghp_abc123xyz456789012345678901234567890'],
        },
      };
      const sanitized = sanitizeCallToolResult(result);
      const items = (sanitized.structuredContent as Record<string, unknown>)
        .items as string[];
      expect(items[0]).toBe('safe');
      expect(items[1]).not.toContain(
        'ghp_abc123xyz456789012345678901234567890'
      );
    });

    it('should preserve isError flag', () => {
      const result: CallToolResult = {
        content: [{ type: 'text', text: 'error occurred' }],
        isError: true,
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.isError).toBe(true);
    });

    it('should handle empty content array', () => {
      const result: CallToolResult = { content: [] };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.content).toEqual([]);
    });

    it('should handle result with no structuredContent', () => {
      const result: CallToolResult = {
        content: [{ type: 'text', text: 'ok' }],
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(sanitized.structuredContent).toBeUndefined();
    });

    it('should handle multiple content items with mixed types', () => {
      const result: CallToolResult = {
        content: [
          { type: 'text', text: 'ghp_abc123xyz456789012345678901234567890' },
          {
            type: 'image' as const,
            data: 'base64',
            mimeType: 'image/png',
          },
          { type: 'text', text: 'clean text' },
        ],
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(
        (sanitized.content[0] as { type: 'text'; text: string }).text
      ).not.toContain('ghp_abc123');
      expect(sanitized.content[1]).toHaveProperty('type', 'image');
      expect(
        (sanitized.content[2] as { type: 'text'; text: string }).text
      ).toBe('clean text');
    });

    it('should sanitize both content and structuredContent simultaneously', () => {
      const result: CallToolResult = {
        content: [
          {
            type: 'text',
            text: 'key=ghp_abc123xyz456789012345678901234567890',
          },
        ],
        structuredContent: {
          token: 'ghp_def456abc7890123456789012345678901ab',
        },
      };
      const sanitized = sanitizeCallToolResult(result);
      expect(
        (sanitized.content[0] as { type: 'text'; text: string }).text
      ).not.toContain('ghp_abc123');
      expect(
        (sanitized.structuredContent as Record<string, unknown>).token
      ).not.toContain('ghp_def456');
    });
  });

  describe('withOutputSanitization', () => {
    let server: McpServer;
    let capturedCb: (...args: unknown[]) => Promise<CallToolResult>;
    let proxy: McpServer;

    beforeEach(() => {
      server = {
        registerTool: vi.fn((_name: string, _config: unknown, cb: unknown) => {
          capturedCb = cb as typeof capturedCb;
          return {} as never;
        }),
      } as unknown as McpServer;
      proxy = withOutputSanitization(server);
    });

    it('should return a proxy, not the original server', () => {
      expect(proxy).not.toBe(server);
    });

    it('should delegate to the original registerTool', () => {
      const handler = vi.fn();
      proxy.registerTool('testTool', {} as never, handler as never);
      expect(server.registerTool).toHaveBeenCalledOnce();
    });

    it('should not replace registerTool on the original server', () => {
      const originalFn = server.registerTool;
      withOutputSanitization(server);
      expect(server.registerTool).toBe(originalFn);
    });

    it('should sanitize the callback return value', async () => {
      const rawHandler = vi.fn().mockResolvedValue({
        content: [
          {
            type: 'text',
            text: 'secret: ghp_abc123xyz456789012345678901234567890',
          },
        ],
      } satisfies CallToolResult);

      proxy.registerTool('myTool', {} as never, rawHandler as never);
      const result = await capturedCb({ query: 'test' });
      expect(
        (result.content[0] as { type: 'text'; text: string }).text
      ).not.toContain('ghp_abc123');
      expect(rawHandler).toHaveBeenCalledOnce();
    });

    it('should sanitize structuredContent from the callback', async () => {
      const rawHandler = vi.fn().mockResolvedValue({
        content: [],
        structuredContent: {
          key: 'ghp_abc123xyz456789012345678901234567890',
        },
      } satisfies CallToolResult);

      proxy.registerTool('myTool', {} as never, rawHandler as never);
      const result = await capturedCb({});
      expect(
        (result.structuredContent as Record<string, unknown>).key
      ).not.toContain('ghp_abc123');
    });

    it('should forward all arguments to the original callback', async () => {
      const rawHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text', text: 'ok' }],
      } satisfies CallToolResult);

      proxy.registerTool('myTool', {} as never, rawHandler as never);
      const args = { query: 'test' };
      const extra = { authInfo: { sub: 'user' }, sessionId: 'sid-1' };
      await capturedCb(args, extra);
      expect(rawHandler).toHaveBeenCalledWith(args, extra);
    });

    it('should pass through clean results without modification', async () => {
      const cleanResult: CallToolResult = {
        content: [{ type: 'text', text: 'hello' }],
        isError: false,
      };
      const rawHandler = vi.fn().mockResolvedValue(cleanResult);

      proxy.registerTool('myTool', {} as never, rawHandler as never);
      const result = await capturedCb({});
      expect(result.content).toEqual(cleanResult.content);
      expect(result.isError).toBe(false);
    });

    it('should preserve tool name and config passed to registerTool', () => {
      const config = { description: 'My tool', inputSchema: {} };
      proxy.registerTool('myTool', config as never, (() => {}) as never);

      expect(server.registerTool).toHaveBeenCalledWith(
        'myTool',
        config,
        expect.any(Function)
      );
    });

    it('should proxy other properties transparently', () => {
      const serverWithExtra = {
        ...server,
        someOtherProp: 42,
      } as unknown as McpServer;
      const p = withOutputSanitization(serverWithExtra);
      expect((p as unknown as { someOtherProp: number }).someOtherProp).toBe(
        42
      );
    });

    describe('resource registration', () => {
      let server: McpServer;
      let captured: Record<string, (...args: unknown[]) => Promise<unknown>> =
        {};
      let proxy: McpServer;

      beforeEach(() => {
        captured = {};
        server = {
          registerTool: vi.fn(),
          registerResource: vi.fn(
            (
              _name: string,
              _uriOrTemplate: unknown,
              _config: unknown,
              cb: unknown
            ) => {
              captured.resource = cb as (...a: unknown[]) => Promise<unknown>;
              return {} as never;
            }
          ),
        } as unknown as McpServer;
        proxy = withOutputSanitization(server);
      });

      it('forwards a successful resource result unchanged', async () => {
        const ok = {
          contents: [
            { uri: 'file:///x', mimeType: 'text/plain', text: 'hello' },
          ],
        };
        const handler = vi.fn().mockResolvedValue(ok);
        proxy.registerResource(
          'doc',
          'file:///x',
          {} as never,
          handler as never
        );
        const result = await captured.resource!(new URL('file:///x'), {});
        expect(result).toEqual(ok);
      });

      it('re-throws a sanitized McpError when a resource handler throws', async () => {
        const handler = vi.fn().mockRejectedValue(new Error('disk on fire'));
        proxy.registerResource(
          'doc',
          'file:///x',
          {} as never,
          handler as never
        );

        await expect(
          captured.resource!(new URL('file:///x'), {})
        ).rejects.toMatchObject({
          message: expect.stringContaining('resource "doc" failed'),
        });
      });
    });

    describe('crash isolation', () => {
      it('should convert an async-thrown Error into isError response', async () => {
        const failing = vi
          .fn()
          .mockRejectedValue(new Error('boom: things went sideways'));

        proxy.registerTool('failingTool', {} as never, failing as never);
        const result = await capturedCb({ query: 'x' });

        expect(result.isError).toBe(true);
        expect(
          (result.content[0] as { type: 'text'; text: string }).text
        ).toContain('failingTool');
        expect(
          (result.content[0] as { type: 'text'; text: string }).text
        ).toContain('boom: things went sideways');
        const structured = result.structuredContent as Record<string, unknown>;
        expect(structured.status).toBe('error');
        expect(structured.tool).toBe('failingTool');
        expect(structured.code).toBe('TOOL_CALLBACK_EXCEPTION');
      });

      it('should catch a synchronously-thrown Error', async () => {
        const failing = vi.fn().mockImplementation(() => {
          throw new TypeError('sync explode');
        });

        proxy.registerTool('syncFail', {} as never, failing as never);
        const result = await capturedCb({});

        expect(result.isError).toBe(true);
        const structured = result.structuredContent as {
          error: { name: string; message: string };
        };
        expect(structured.error.name).toBe('TypeError');
        expect(structured.error.message).toBe('sync explode');
      });

      it('should handle non-Error rejections (string)', async () => {
        const failing = vi.fn().mockRejectedValue('bare string failure');

        proxy.registerTool('stringThrow', {} as never, failing as never);
        const result = await capturedCb({});

        expect(result.isError).toBe(true);
        const structured = result.structuredContent as {
          error: { message: string };
        };
        expect(structured.error.message).toBe('bare string failure');
      });

      it('should handle non-Error rejections (plain object with message)', async () => {
        const failing = vi
          .fn()
          .mockRejectedValue({ message: 'object failure', code: 'E_OBJ' });

        proxy.registerTool('objThrow', {} as never, failing as never);
        const result = await capturedCb({});

        expect(result.isError).toBe(true);
        const structured = result.structuredContent as {
          error: { message: string; code?: string };
        };
        expect(structured.error.message).toBe('object failure');
        expect(structured.error.code).toBe('E_OBJ');
      });

      it('should handle null/undefined rejections', async () => {
        const failing = vi.fn().mockRejectedValue(undefined);

        proxy.registerTool('undefThrow', {} as never, failing as never);
        const result = await capturedCb({});

        expect(result.isError).toBe(true);
        const structured = result.structuredContent as {
          error: { message: string };
        };
        expect(typeof structured.error.message).toBe('string');
      });

      it('should redact secrets present in the thrown error message', async () => {
        const failing = vi
          .fn()
          .mockRejectedValue(
            new Error('token=ghp_abc123xyz456789012345678901234567890 leaked')
          );

        proxy.registerTool('leakyFail', {} as never, failing as never);
        const result = await capturedCb({});

        expect(result.isError).toBe(true);
        const text = (result.content[0] as { type: 'text'; text: string }).text;
        expect(text).not.toContain('ghp_abc123xyz456789012345678901234567890');
      });

      it('should never re-throw — MCP transport receives a CallToolResult', async () => {
        const failing = vi.fn().mockRejectedValue(new Error('still safe'));

        proxy.registerTool('safeFail', {} as never, failing as never);
        await expect(capturedCb({})).resolves.toBeDefined();
      });
    });
  });

  describe('buildToolErrorResult', () => {
    it('should produce a sanitized error result for arbitrary throws', () => {
      const result = buildToolErrorResult(
        'someTool',
        new Error('with token ghp_abc123xyz456789012345678901234567890')
      );

      expect(result.isError).toBe(true);
      const text = (result.content[0] as { type: 'text'; text: string }).text;
      expect(text).toContain('someTool');
      expect(text).not.toContain('ghp_abc123xyz456789012345678901234567890');
      const structured = result.structuredContent as {
        status: string;
        tool: string;
        code: string;
      };
      expect(structured.status).toBe('error');
      expect(structured.tool).toBe('someTool');
      expect(structured.code).toBe('TOOL_CALLBACK_EXCEPTION');
    });

    it('should normalize numeric throws into a message', () => {
      const result = buildToolErrorResult('t', 42);
      expect(result.isError).toBe(true);
      const structured = result.structuredContent as {
        error: { message: string };
      };
      expect(structured.error.message).toBe('42');
    });
  });
});
