/**
 * secureServer.coverage.test.ts
 *
 * Covers the paths NOT exercised by secureServer.real.test.ts:
 *  - wrapNonToolCallback (registerResource success + error)
 *  - normalizeError branches: string, plain-object, undefined, object-without-message
 *  - safeStringify circular-reference path
 *  - registerResource proxy branch in withOutputSanitization
 */
import { describe, it, expect } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { withOutputSanitization } from '../../src/utils/secureServer.js';

async function setupPair() {
  const server = new McpServer({ name: 'test-server', version: '0.0.0' });
  const secure = withOutputSanitization(server);
  const client = new Client({ name: 'test-client', version: '0.0.0' });
  const [serverTransport, clientTransport] =
    InMemoryTransport.createLinkedPair();
  return { server, secure, client, serverTransport, clientTransport };
}

describe('secureServer — registerResource proxy (wrapNonToolCallback)', () => {
  it('passes through a successful resource read', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerResource(
      'static-doc',
      'file:///static-doc',
      { description: 'A static document' },
      async () => ({
        contents: [
          {
            uri: 'file:///static-doc',
            text: 'hello resource',
            mimeType: 'text/plain',
          },
        ],
      })
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.readResource({ uri: 'file:///static-doc' });
    expect(result.contents[0]).toMatchObject({ text: 'hello resource' });

    await client.close();
    await server.close();
  });

  it('converts a thrown Error in a resource handler to McpError', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerResource(
      'failing-doc',
      'file:///failing-doc',
      { description: 'Always fails' },
      async () => {
        throw new Error('resource handler boom');
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///failing-doc' })
    ).rejects.toMatchObject({
      message: expect.stringContaining('failing-doc'),
    });

    await client.close();
    await server.close();
  });

  it('converts a thrown string in a resource handler to McpError', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    (secure as any).registerResource(
      'string-error-doc',
      'file:///string-error-doc',
      { description: 'Throws a string' },
      async () => {
        // normalizeError string branch
        throw 'plain string error';
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///string-error-doc' })
    ).rejects.toMatchObject({
      message: expect.stringContaining('string-error-doc'),
    });

    await client.close();
    await server.close();
  });

  it('converts a thrown plain object (no message) to McpError using safeStringify', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    (secure as any).registerResource(
      'obj-error-doc',
      'file:///obj-error-doc',
      { description: 'Throws a plain object without message' },
      async () => {
        // normalizeError object-without-message branch → safeStringify
        throw { code: 'ERR_NO_MESSAGE', status: 503 };
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///obj-error-doc' })
    ).rejects.toMatchObject({
      message: expect.stringContaining('obj-error-doc'),
    });

    await client.close();
    await server.close();
  });
});

describe('secureServer — Proxy Reflect.get fallthrough', () => {
  it('passes through non-intercepted properties unchanged', () => {
    const server = new McpServer({ name: 'test-server', version: '0.0.0' });
    const secure = withOutputSanitization(server);
    // 'connect' is not 'registerTool' nor 'registerResource' — exercises Reflect.get branch
    expect(typeof (secure as unknown as Record<string, unknown>).connect).toBe(
      'function'
    );
    expect(typeof (secure as unknown as Record<string, unknown>).name).toBe(
      'undefined'
    );
  });
});

describe('secureServer — normalizeError edge cases via resource handler throws', () => {
  it('handles thrown object WITH a message field (covers normalizeError object+message branch)', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerResource(
      'obj-msg-res',
      'file:///obj-msg-res',
      { description: 'throws object with message' },
      async () => {
        // normalizeError: object branch, obj.message is string → line 42
        throw {
          name: 'CustomError',
          message: 'detailed failure',
          code: 'E_CUSTOM',
        };
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///obj-msg-res' })
    ).rejects.toMatchObject({
      message: expect.stringContaining('obj-msg-res'),
    });

    await client.close();
    await server.close();
  });

  it('handles thrown undefined from a resource handler (null/undefined fallback branch)', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerResource(
      'undef-res',
      'file:///undef-res',
      { description: 'throws undefined' },
      async () => {
        // normalizeError final return: error === undefined → 'undefined'
        throw undefined;
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///undef-res' })
    ).rejects.toMatchObject({ message: expect.stringContaining('undef-res') });

    await client.close();
    await server.close();
  });

  it('handles thrown circular object (safeStringify returns undefined, hits ?? fallback)', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerResource(
      'circular-res',
      'file:///circular-res',
      { description: 'throws circular reference object' },
      async () => {
        // safeStringify catch → return undefined → ?? 'Unknown error'
        const circular: Record<string, unknown> = { code: 'CIRC' };
        circular.self = circular;
        throw circular;
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    await expect(
      client.readResource({ uri: 'file:///circular-res' })
    ).rejects.toMatchObject({
      message: expect.stringContaining('circular-res'),
    });

    await client.close();
    await server.close();
  });
});

describe('secureServer — normalizeError edge cases via tool throws', () => {
  it('handles undefined throw in a tool callback', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerTool(
      'undef-throw',
      { description: 'throws undefined', inputSchema: {} },
      async () => {
        // normalizeError: error === undefined branch
        throw undefined;
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: 'undef-throw',
      arguments: {},
    });
    expect(result.isError).toBe(true);

    await client.close();
    await server.close();
  });

  it('handles null throw in a tool callback', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerTool(
      'null-throw',
      { description: 'throws null', inputSchema: {} },
      async () => {
        // normalizeError: String(null) = 'null'
        throw null;
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({ name: 'null-throw', arguments: {} });
    expect(result.isError).toBe(true);

    await client.close();
    await server.close();
  });

  it('handles plain-object throw with a message field', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerTool(
      'obj-msg-throw',
      { description: 'throws object with message', inputSchema: {} },
      async () => {
        // normalizeError: object branch, obj.message is string
        throw {
          name: 'CustomError',
          message: 'structured failure',
          code: 'E_CUSTOM',
        };
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: 'obj-msg-throw',
      arguments: {},
    });
    expect(result.isError).toBe(true);

    await client.close();
    await server.close();
  });

  it('handles circular-reference object throw (safeStringify returns undefined fallback)', async () => {
    const { server, secure, client, serverTransport, clientTransport } =
      await setupPair();

    secure.registerTool(
      'circular-throw',
      { description: 'throws circular object', inputSchema: {} },
      async () => {
        // safeStringify catch branch: circular reference → JSON.stringify throws
        const circular: Record<string, unknown> = { code: 'CIRC' };
        circular.self = circular;
        throw circular;
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: 'circular-throw',
      arguments: {},
    });
    expect(result.isError).toBe(true);

    await client.close();
    await server.close();
  });
});
