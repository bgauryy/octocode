import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import {
  getNativeContractFingerprint,
  TOOL_NAMES,
} from '@octocodeai/config/schema';

// startNativeMcp constructs a real StdioServerTransport (reads process.stdin and
// writes process.stdout). Stub it so the transport lifecycle is exercised without
// touching real stdio.
vi.mock('@modelcontextprotocol/server/stdio', () => ({
  StdioServerTransport: class {
    async start(): Promise<void> {}
    async send(): Promise<void> {}
    async close(): Promise<void> {}
  },
}));

import {
  createNativeMcp,
  loadNativeBinding,
  startNativeMcp,
  type NativeCatalog,
  type NativeCatalogTool,
  type NativeRuntime,
  type NativeRuntimeOptions,
} from '../../src/native/index.js';

const fixture = (name: string) =>
  fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));

// Exercises the src-level createNativeMcp registration/execution path through a
// real McpServer + in-memory client. Ports the un-gated tests/native/node-
// boundary.mjs smoke test into the gated vitest suite so the registration loop,
// the `available` filter, the per-tool execute callback, the missing-contract
// guard, and idempotent close are all covered by `yarn test` (they previously
// were not — src/native/index.ts sat at ~49% and failed the coverage gate).

interface RecordedExecution {
  requestId: string;
  tool: string;
  input: unknown;
}

// Matches NATIVE_ABI_VERSION without importing @octocodeai/octocode-native/runtime
// (that import loads the native addon — slow, and it races a native rebuild).
// The sibling native tests (fingerprint-drift.test.ts, node-boundary.mjs) pin
// the same literal; they move together if the ABI is ever bumped.
const FAKE_ABI_VERSION = 2;

class FakeRuntime implements NativeRuntime {
  static last: FakeRuntime | undefined;
  readonly abiVersion: number = FAKE_ABI_VERSION;
  readonly executions: RecordedExecution[] = [];
  readonly options: NativeRuntimeOptions | undefined;
  closed = false;
  closeCount = 0;

  constructor(
    private readonly makeCatalog: () => NativeCatalog,
    options?: NativeRuntimeOptions
  ) {
    FakeRuntime.last = this;
    this.options = options;
  }

  catalog(): NativeCatalog {
    return this.makeCatalog();
  }

  cancel(): boolean {
    return true;
  }

  async executeMcp(
    requestId: string,
    tool: string,
    input: unknown
  ): Promise<unknown> {
    this.executions.push({ requestId, tool, input });
    return {
      content: [{ type: 'text', text: 'ok' }],
      structuredContent: { results: [{ index: 0, data: { ok: true } }] },
      isError: false,
    };
  }

  async close(): Promise<void> {
    this.closed = true;
    this.closeCount += 1;
  }
}

const bindingFor = (makeCatalog: () => NativeCatalog) => ({
  NativeRuntime: class extends FakeRuntime {
    constructor(options?: NativeRuntimeOptions) {
      super(makeCatalog, options);
    }
  },
});

const tool = (
  name: string,
  available: boolean,
  extra: Partial<NativeCatalogTool> = {}
): NativeCatalogTool => ({
  name,
  available,
  ...extra,
});

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp registration + execution', () => {
  it('gives MCP executions headroom beyond the worst cold LSP budget', async () => {
    const instance = createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });

    expect(FakeRuntime.last?.options).toMatchObject({
      surface: 'mcp',
      timeoutSecs: 300,
      env: {},
    });

    await instance.close();
  });

  it.each([
    ['missing', undefined, false],
    ['blank', '   ', false],
  ])(
    'omits clasify from discovery when OCTOCODE_CLASSIFICATION_API is %s',
    async (_label, credential, nativeAvailable) => {
      // Native owns credential resolution. This fixture supplies the catalog
      // availability state that the adapter must honor without reinterpreting it.
      const instance = createNativeMcp({
        env: { OCTOCODE_CLASSIFICATION_API: credential },
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [
            tool('localFetch', true),
            tool(TOOL_NAMES.CLASIFY, nativeAvailable),
          ],
        })),
      });

      const client = new Client({ name: 'semantic-gate', version: '1' });
      const [serverTransport, clientTransport] =
        InMemoryTransport.createLinkedPair();
      await Promise.all([
        instance.server.connect(serverTransport),
        client.connect(clientTransport),
      ]);

      const list = await client.listTools();
      expect(list.tools.map(t => t.name)).toEqual(['localFetch']);
      expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(
        true
      );

      await client.close();
      await instance.close();
    }
  );

  it('exposes clasify when OCTOCODE_CLASSIFICATION_API is nonblank', async () => {
    const instance = createNativeMcp({
      env: {
        OCTOCODE_CLASSIFICATION_API: 'test-key',
      },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [
          tool('localFetch', true),
          tool(TOOL_NAMES.CLASIFY, true),
        ],
      })),
    });

    const client = new Client({ name: 'semantic-gate', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const list = await client.listTools();
    expect(list.tools.map(t => t.name)).toEqual([
      'localFetch',
      TOOL_NAMES.CLASIFY,
    ]);
    expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(true);

    await client.close();
    await instance.close();
  });

  it('registers only available tools and routes calls to executeMcp', async () => {
    const instance = createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool('ghSearch', false)],
      })),
    });

    const client = new Client({ name: 'create-native-mcp', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    // Only the available tool is advertised (ghSearch.available === false).
    const list = await client.listTools();
    expect(list.tools.map(t => t.name)).toEqual(['localFetch']);
    expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(true);

    // Calling the tool drives the registered async callback → runtime.executeMcp.
    // The adapter does not advertise an output schema, so assert on the recorded
    // execution and returned envelope to prove structured results remain intact.
    const response = await client.callTool({
      name: 'localFetch',
      arguments: {
        queries: [{ reasoning: 'boundary test', path: '.', fullContent: true }],
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

  it('throws and closes the runtime when a catalog tool has no core contract', () => {
    expect(() =>
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('not-a-real-octocode-tool', true)],
        })),
      })
    ).toThrow(/no contract/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when no tools are available', () => {
    expect(() =>
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('localFetch', false)],
        })),
      })
    ).toThrow(/No native tools are available/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime on an ABI mismatch', () => {
    const binding = {
      NativeRuntime: class extends FakeRuntime {
        readonly abiVersion = 999999;
        constructor() {
          super(() => ({
            fingerprint: getNativeContractFingerprint(),
            tools: [tool('localFetch', true)],
          }));
        }
      },
    };
    expect(() => createNativeMcp({ env: {}, binding })).toThrow(/ABI/);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when the catalog exposes no fingerprint', () => {
    expect(() =>
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: '',
          tools: [tool('localFetch', true)],
        })),
      })
    ).toThrow(/does not expose a contract fingerprint/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });
});

describe('loadNativeBinding', () => {
  it('resolves a candidate addon that exports NativeRuntime', () => {
    const binding = loadNativeBinding({
      OCTOCODE_NATIVE_BINDING: fixture('fake-addon.cjs'),
    });
    expect(typeof binding.NativeRuntime).toBe('function');
  });

  it('throws when the candidate addon does not export NativeRuntime', () => {
    expect(() =>
      loadNativeBinding({
        OCTOCODE_NATIVE_BINDING: fixture('addon-without-runtime.cjs'),
      })
    ).toThrow(/does not export NativeRuntime/i);
  });
});

describe('startNativeMcp', () => {
  it('connects a transport, returns the instance, and registers shutdown handlers', async () => {
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
      expect(instance.server).toBeDefined();
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
