import { afterEach, describe, expect, it, vi } from 'vitest';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';

// startNativeMcp serves real stdio (reads process.stdin and writes
// process.stdout). Stub the entry so the lifecycle is exercised without
// touching real stdio; the stub records the server factory it was given.
const serveStdio = vi.hoisted(() =>
  vi.fn((_factory: () => unknown) => ({ close: async () => {} }))
);
vi.mock('@modelcontextprotocol/server/stdio', () => ({ serveStdio }));

import { createNativeMcp, startNativeMcp } from '../../src/native/index.js';
import {
  bindingFor,
  connectClient,
  FakeRuntime,
  nativeResult,
  tool,
} from './native-mcp-fixture.js';

// Exercises the src-level createNativeMcp registration/execution path through a
// real McpServer + in-memory client. Ports the un-gated tests/native/node-
// boundary.mjs smoke test into the gated vitest suite so the registration loop,
// the `available` filter, the per-tool execute callback, the missing-contract
// guard, and idempotent close are all covered by `yarn test` (they previously
// were not — src/native/index.ts sat at ~49% and failed the coverage gate).

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp registration + execution', () => {
  it('registers only available tools and routes calls to executeMcp', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [
          tool('localFetch', true),
          tool('ghSearchCode', false),
          // CLI-only: native resolves it unavailable on the MCP surface.
          tool('ghCloneRepo', false),
        ],
      })),
    });

    const client = await connectClient(instance, 'create-native-mcp');

    // Only what native reports available is loaded.
    const list = await client.listTools();
    expect(list.tools.map(t => t.name)).toEqual(['localFetch']);
    expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(true);
    expect(client.getInstructions()).toBe(
      buildMcpInstructions({ availableTools: ['localFetch'] })
    );

    // Calling the tool drives the registered async callback → runtime.executeMcp.
    // The adapter does not advertise an output schema, so assert on the recorded
    // execution and returned envelope to prove structured results remain intact.
    const response = await client.callTool({
      name: 'localFetch',
      arguments: {
        queries: [
          {
            mainGoal: 'test goal',
            reasoning: 'boundary test',
            path: '.',
            fullContent: true,
          },
        ],
      },
    });
    expect(response.structuredContent).toEqual({
      results: [{ index: 0, data: { ok: true } }],
    });

    const runtime = FakeRuntime.last!;
    expect(runtime.executions).toHaveLength(1);
    expect(runtime.executions[0]!.tool).toBe('localFetch');
    // The SDK parses args against the core inputSchema (which may inject
    // defaults such as debug:false), so assert the meaningful fields rather
    // than an exact envelope.
    const input = runtime.executions[0]!.input as {
      queries: Array<Record<string, unknown>>;
    };
    expect(input.queries).toHaveLength(1);
    expect(input.queries[0]!.reasoning).toBe('boundary test');
    expect(input.queries[0]!.path).toBe('.');
    expect(typeof runtime.executions[0]!.requestId).toBe('string');
    expect(runtime.executions[0]!.requestId.length).toBeGreaterThan(0);

    await client.close();
    // close() is memoized: repeated calls must not re-close the runtime.
    await instance.close();
    await instance.close();
    expect(runtime.closed).toBe(true);
    expect(runtime.closeCount).toBe(1);
  });
  it('keeps numeric and string JSON-RPC ids independent during concurrent execution', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    const replies: Array<{ id: string | number; result: unknown }> = [];
    clientTransport.onmessage = message => {
      if ('id' in message && 'result' in message) replies.push(message);
    };
    const runtime = FakeRuntime.last!;
    try {
      await Promise.all([
        instance.createServer().connect(serverTransport),
        clientTransport.start(),
      ]);
      await clientTransport.send({
        jsonrpc: '2.0',
        id: 'initialize',
        method: 'initialize',
        params: {
          protocolVersion: '2025-06-18',
          capabilities: {},
          clientInfo: { name: 'request-id-types', version: '1' },
        },
      });
      await vi.waitFor(() =>
        expect(replies.some(reply => reply.id === 'initialize')).toBe(true)
      );
      await clientTransport.send({
        jsonrpc: '2.0',
        method: 'notifications/initialized',
      });
      runtime.hang = true;
      await clientTransport.send({
        jsonrpc: '2.0',
        id: 7,
        method: 'tools/call',
        params: {
          name: 'localFetch',
          arguments: { queries: [{ path: 'first.txt' }] },
        },
      });
      await vi.waitFor(() => expect(runtime.executions).toHaveLength(1));
      runtime.hang = false;
      await clientTransport.send({
        jsonrpc: '2.0',
        id: '7',
        method: 'tools/call',
        params: {
          name: 'localFetch',
          arguments: { queries: [{ path: 'second.txt' }] },
        },
      });
      await vi.waitFor(() => expect(runtime.executions).toHaveLength(2));
      await vi.waitFor(() =>
        expect(replies.find(reply => reply.id === '7')?.result).toMatchObject({
          isError: false,
        })
      );
      const [first, second] = runtime.executions;
      expect(first!.requestId).not.toBe(second!.requestId);
      expect(first!.input).toEqual({ queries: [{ path: 'first.txt' }] });
      expect(second!.input).toEqual({ queries: [{ path: 'second.txt' }] });
      await clientTransport.send({
        jsonrpc: '2.0',
        method: 'notifications/cancelled',
        params: { requestId: 7, reason: 'Cancel only the first request.' },
      });
      await vi.waitFor(() =>
        expect(runtime.cancelled).toEqual([first!.requestId])
      );
      expect(runtime.cancelled).not.toContain(second!.requestId);
    } finally {
      await clientTransport.send({
        jsonrpc: '2.0',
        method: 'notifications/cancelled',
        params: { requestId: 7, reason: 'Fixture cleanup.' },
      });
      await clientTransport.close();
      await instance.close();
    }
  });

  describe('native execution errors', () => {
    async function call(failure: unknown, result?: unknown) {
      const stderr = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);
      const instance = await createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('localFetch', true)],
        })),
      });
      const client = await connectClient(instance, 'native-errors');
      FakeRuntime.last!.failure = failure;
      FakeRuntime.last!.result = result;
      try {
        return await client.callTool({
          name: 'localFetch',
          arguments: {
            queries: [{ mainGoal: 'g', reasoning: 'r', path: '.' }],
          },
        });
      } finally {
        stderr.mockRestore();
        await client.close();
        await instance.close();
      }
    }

    // Native projects every rejected call (timeout, busy, unavailable tool,
    // invalid input) into the tool error the CLI prints; the adapter passes
    // it through untouched.
    it('passes the native tool error through unchanged', async () => {
      const rejected = nativeResult(
        {
          error: 'Tool call timed out.',
          details: ['Retry, or narrow the call.'],
          errorCode: 'timeout',
        },
        true
      );
      const result = await call(undefined, rejected);
      expect(result).toEqual(rejected);
    });

    // Native rejects only a cancelled request or a closing runtime; any
    // rejection that reaches the client stays generic.
    it.each([
      [
        'a closing runtime',
        new Error(
          JSON.stringify({
            kind: 'octocode.nativeError',
            code: 'closed',
            message: 'The runtime is shutting down.',
            payload: null,
          })
        ),
      ],
      ['a non-native error', new Error('ENOENT /Users/me/.ssh/id_rsa')],
      ['a non-error value', 'boom'],
    ])(
      'keeps %s generic and out of the client text',
      async (_name, failure) => {
        const result = await call(failure);
        expect(result.isError).toBe(true);
        expect(result.structuredContent).toBeUndefined();
        expect(result.content).toEqual([
          {
            type: 'text',
            text: 'Tool localFetch failed to execute; see the server logs for detail.',
          },
        ]);
      }
    );
  });
});

describe('startNativeMcp', () => {
  it('serves stdio from the server factory, returns the instance, and registers shutdown handlers', async () => {
    const sigint = process.listeners('SIGINT').slice();
    const sigterm = process.listeners('SIGTERM').slice();
    const stdinEnd = process.stdin.listeners('end').slice();

    const instance = await startNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });

    const exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation((() => undefined) as never);
    try {
      expect(serveStdio).toHaveBeenCalledWith(
        instance.createServer,
        expect.objectContaining({ onerror: expect.any(Function) })
      );
      expect(process.listeners('SIGINT').length).toBeGreaterThan(sigint.length);
      expect(process.listeners('SIGTERM').length).toBeGreaterThan(
        sigterm.length
      );

      // Invoke the registered shutdown handler directly (rather than emitting a
      // real signal, which would trip other listeners) with process.exit stubbed,
      // so the drain-then-exit path is exercised without killing the test runner.
      const added = process
        .listeners('SIGINT')
        .filter(listener => !sigint.includes(listener)) as Array<() => void>;
      expect(added).toHaveLength(1);
      added[0]!();
      await new Promise(resolve => setImmediate(resolve));
      expect(exitSpy).toHaveBeenCalledWith(0);
    } finally {
      exitSpy.mockRestore();
      await instance.close();
      // Remove only the shutdown listeners this test added so they cannot leak
      // (and their process.exit(0)) into the rest of the suite.
      for (const listener of process.listeners('SIGINT'))
        if (!sigint.includes(listener))
          process.removeListener('SIGINT', listener);
      for (const listener of process.listeners('SIGTERM'))
        if (!sigterm.includes(listener))
          process.removeListener('SIGTERM', listener);
      for (const listener of process.stdin.listeners('end'))
        if (!stdinEnd.includes(listener))
          process.stdin.removeListener('end', listener as () => void);
    }
  });
});

describe('startNativeMcp fatal handlers', () => {
  it('logs uncaught errors to stderr (never stdout), closes the runtime, and exits 1', async () => {
    const before = {
      ue: process.listeners('uncaughtException').slice(),
      ur: process.listeners('unhandledRejection').slice(),
      sigint: process.listeners('SIGINT').slice(),
      sigterm: process.listeners('SIGTERM').slice(),
      end: process.stdin.listeners('end').slice(),
    };
    const instance = await startNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });
    const exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation((() => undefined) as never);
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation((() => true) as never);
    const stdout = vi.spyOn(process.stdout, 'write');
    try {
      const added = process
        .listeners('uncaughtException')
        .filter(l => !before.ue.includes(l)) as Array<(e: unknown) => void>;
      expect(added).toHaveLength(1);
      added[0]!(new Error('boom'));
      await new Promise(resolve => setImmediate(resolve));
      await new Promise(resolve => setImmediate(resolve));
      expect(stderr).toHaveBeenCalledWith(
        expect.stringContaining('uncaughtException')
      );
      expect(stdout).not.toHaveBeenCalled();
      expect(FakeRuntime.last!.closed).toBe(true);
      expect(exitSpy).toHaveBeenCalledWith(1);
    } finally {
      exitSpy.mockRestore();
      stderr.mockRestore();
      stdout.mockRestore();
      await instance.close();
      const strip = (event: string, keep: Function[]) => {
        for (const l of process.listeners(event as never))
          if (!keep.includes(l)) process.removeListener(event, l as never);
      };
      strip('uncaughtException', before.ue);
      strip('unhandledRejection', before.ur);
      strip('SIGINT', before.sigint);
      strip('SIGTERM', before.sigterm);
      for (const l of process.stdin.listeners('end'))
        if (!before.end.includes(l))
          process.stdin.removeListener('end', l as () => void);
    }
  });
});
