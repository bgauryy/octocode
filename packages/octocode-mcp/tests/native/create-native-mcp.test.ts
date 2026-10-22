import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import {
  getDirectToolDefinitionsWithAddons,
  getNativeContractFingerprint,
  isCliOnlyTool,
  TOOL_NAMES,
  TOOL_POLICIES,
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
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';

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

// The fake speaks the ABI the MCP expects; src/native/index.ts already imports
// this module, so importing it here loads nothing extra.
const FAKE_ABI_VERSION = NATIVE_ABI_VERSION;

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

  readonly cancelled: string[] = [];
  hang = false;
  /** When set, `executeMcp` rejects with it. */
  failure: unknown;
  /** When set, `executeMcp` resolves with it. */
  result: unknown;
  providerAvailable = true;

  cancel(requestId: string): boolean {
    this.cancelled.push(requestId);
    return true;
  }

  async executeMcp(
    requestId: string,
    tool: string,
    input: unknown
  ): Promise<unknown> {
    this.executions.push({ requestId, tool, input });
    if (this.failure !== undefined) throw this.failure;
    if (this.result !== undefined) return this.result;
    if (this.hang) {
      await new Promise<void>(resolve => {
        const poll = setInterval(() => {
          if (this.cancelled.includes(requestId)) {
            clearInterval(poll);
            resolve();
          }
        }, 5);
      });
      throw new Error('cancelled');
    }
    if (tool === 'clasify') {
      if (!this.providerAvailable) {
        return {
          content: [
            { type: 'text', text: 'Classification provider returned HTTP 401' },
          ],
          structuredContent: { errorCode: 'classificationProviderError' },
          isError: true,
        };
      }
      // Native clasify receipts carry no rendered text block.
      return {
        content: [],
        structuredContent: { queries: [{ id: 'q1', status: 'ok' }] },
        isError: false,
      };
    }
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
  shortDescription: `${name} description.`,
  available,
  ...extra,
});

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp registration + execution', () => {
  it('leaves the per-request execution budget to native', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });

    expect(FakeRuntime.last?.options).toEqual({ surface: 'mcp', env: {} });

    await instance.close();
  });

  it.each([
    ['missing', undefined, false],
    ['blank', '   ', false],
  ])(
    'omits clasify from discovery with beta enabled when OCTOCODE_CLASSIFICATION_API is %s',
    async (_label, credential, nativeAvailable) => {
      // Native owns beta and credential resolution. This fixture supplies the catalog
      // availability state that the adapter must honor without reinterpreting it.
      const instance = await createNativeMcp({
        env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: credential },
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

  it('exposes clasify when beta and OCTOCODE_CLASSIFICATION_API are enabled', async () => {
    const instance = await createNativeMcp({
      env: {
        OCTOCODE_BETA: '1',
        OCTOCODE_CLASSIFICATION_API: 'test-key',
      },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
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
    // Every core-authored hint survives the MCP registration and transport.
    const authored = getDirectToolDefinitionsWithAddons({
      availableTools: list.tools.map(listed => listed.name),
    });
    for (const listed of list.tools)
      expect(listed.annotations, listed.name).toEqual(
        authored.find(definition => definition.name === listed.name)!
          .annotations
      );

    await client.close();
    await instance.close();
  });

  it('keeps clasify registered through provider failure and later recovery', async () => {
    const instance = await createNativeMcp({
      env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: 'test-key' },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
      })),
    });
    const runtime = FakeRuntime.last!;
    expect(runtime.executions).toHaveLength(0);

    const client = new Client({ name: 'clasify-recovery', version: '1' });
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
    expect(client.getInstructions()).toBe(
      buildMcpInstructions({ availableTools: list.tools.map(t => t.name) })
    );
    const matrix = {
      mainGoal: 'Check provider recovery.',
      reasoning: 'The same host must retain the public tool.',
      resources: [{ value: 'fixture' }],
      questions: [{ type: 'yesno', ask: 'Is this a fixture?' }],
    };
    runtime.providerAvailable = false;
    const failed = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(failed.isError).toBe(true);
    expect(failed.structuredContent).toMatchObject({
      errorCode: 'classificationProviderError',
    });
    runtime.providerAvailable = true;
    const recovered = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(recovered.isError).toBe(false);
    expect(runtime.executions).toHaveLength(2);

    await client.close();
    await instance.close();
  });

  it('accepts a clasify queries[] matrix and normalizes it before execution', async () => {
    const instance = await createNativeMcp({
      env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: 'test-key' },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
      })),
    });
    const client = new Client({ name: 'clasify-queries', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const matrix = {
      mainGoal: 'test goal',
      reasoning: 'Locate one fact without caller-authored IDs.',
      resources: [{ value: 'captured source' }],
      questions: [{ type: 'locate', ask: 'The fact to locate' }],
    };
    const response = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(response.isError).toBe(false);
    expect(FakeRuntime.last?.executions.at(-1)).toMatchObject({
      tool: TOOL_NAMES.CLASIFY,
      input: { queries: [matrix] },
    });
    // Text-only clients must not receive an empty content array.
    expect(response.content).toEqual([
      { type: 'text', text: JSON.stringify(response.structuredContent) },
    ]);

    await client.close();
    await instance.close();
  });

  it.each([
    ['disabled', false],
    ['enabled', true],
  ])(
    'keeps semantic checks in clasify when provider capability is %s',
    async (_label, clasifyAvailable) => {
      const instance = await createNativeMcp({
        env: clasifyAvailable
          ? {
              OCTOCODE_BETA: '1',
              OCTOCODE_CLASSIFICATION_API: 'test-key',
              OCTOCODE_CLASSIFICATION_API_HOST:
                'https://classification.example.test',
            }
          : {
              OCTOCODE_BETA: '1',
              OCTOCODE_CLASSIFICATION_API_HOST:
                'https://classification.example.test',
            },
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [
            tool('ghSearchCode', true),
            tool('localSearch', true),
            tool(TOOL_NAMES.CLASIFY, clasifyAvailable),
          ],
        })),
      });
      const client = new Client({
        name: 'semantic-rerank-addon',
        version: '1',
      });
      const [serverTransport, clientTransport] =
        InMemoryTransport.createLinkedPair();
      await Promise.all([
        instance.server.connect(serverTransport),
        client.connect(clientTransport),
      ]);
      const listed = await client.listTools();
      // Shared guidance belongs to initialize.instructions, never each tool.
      const sharedOpening = client.getInstructions()?.split('\n')[0];
      expect(sharedOpening).toBeTruthy();
      for (const registered of listed.tools) {
        expect(registered.description).toBeTruthy();
        expect(registered.description).not.toContain(sharedOpening!);
      }
      expect(
        listed.tools.some(candidate => candidate.name === TOOL_NAMES.CLASIFY)
      ).toBe(clasifyAvailable);
      for (const name of ['ghSearchCode', 'localSearch']) {
        const schema = JSON.stringify(
          listed.tools.find(candidate => candidate.name === name)?.inputSchema
        );
        expect(schema.includes('semanticRerank')).toBe(false);
        expect(schema).not.toContain('minScore');
        expect(schema).not.toContain('clasifyContext');
        expect(schema).not.toContain('clasifyMinScore');
      }
      await client.close();
      await instance.close();
    }
  );

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

    const client = new Client({ name: 'create-native-mcp', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

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

  it('forwards client cancellation (notifications/cancelled) with the admitted native request id', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });
    const client = new Client({ name: 'cancel', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);
    const runtime = FakeRuntime.last!;
    runtime.hang = true;
    const controller = new AbortController();
    const call = client
      .callTool(
        {
          name: 'localFetch',
          arguments: {
            queries: [
              { mainGoal: 'test goal', reasoning: 'cancel test', path: '.' },
            ],
          },
        },
        undefined,
        { signal: controller.signal }
      )
      .catch((error: unknown) => error);
    await vi.waitFor(() => expect(runtime.executions).toHaveLength(1));
    controller.abort();
    await call;
    await vi.waitFor(() => expect(runtime.cancelled).toHaveLength(1));
    expect(runtime.cancelled[0]).toBe(runtime.executions[0]!.requestId);
    await client.close();
    await instance.close();
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
        instance.server.connect(serverTransport),
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
      const client = new Client({ name: 'native-errors', version: '1' });
      const [serverTransport, clientTransport] =
        InMemoryTransport.createLinkedPair();
      await Promise.all([
        instance.server.connect(serverTransport),
        client.connect(clientTransport),
      ]);
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
      const nativeResult = {
        content: [
          {
            type: 'text',
            text: 'Tool call timed out.; Retry, or narrow the call. (errorCode: timeout)',
          },
        ],
        structuredContent: {
          error: 'Tool call timed out.',
          details: ['Retry, or narrow the call.'],
          errorCode: 'timeout',
        },
        isError: true,
      };
      const result = await call(undefined, nativeResult);
      expect(result).toEqual(nativeResult);
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

  it('registers only the tools native reports available on MCP', async () => {
    const cliOnly = Object.keys(TOOL_POLICIES).filter(isCliOnlyTool);
    expect(cliOnly.length).toBeGreaterThan(0);
    // Native resolves CLI-only tools as unavailable on the MCP surface.
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [
          tool('localFetch', true),
          ...cliOnly.map(name => tool(name, false)),
        ],
      })),
    });
    const client = new Client({ name: 'cli-only', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);
    expect((await client.listTools()).tools.map(t => t.name)).toEqual([
      'localFetch',
    ]);
    await client.close();
    await instance.close();
  });

  // Hosts truncate server instructions near 2 KB; the grammar inventory is
  // served by `octocode schema`, not the MCP instructions.
  it('keeps the grammar inventory out of the size-bounded MCP instructions', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('astSearch', true)],
        grammarCapabilities: [
          {
            language: 'Rust',
            languageId: 'rust',
            selectorAliases: [],
            extensions: ['rs'],
            structuralSearch: true,
            signatureOutline: true,
            graphFacts: true,
          },
        ],
      })),
    });
    const client = new Client({ name: 'grammar-inventory', version: '1' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      instance.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);
    expect(client.getInstructions()).not.toContain('Runtime grammar inventory');
    expect(client.getInstructions()).toBe(
      buildMcpInstructions({ availableTools: ['astSearch'] })
    );
    expect(client.getInstructions()!.length).toBeLessThanOrEqual(2_000);
    await client.close();
    await instance.close();
  });

  it('throws and closes the runtime when a catalog tool has no core contract', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('not-a-real-octocode-tool', true)],
        })),
      })
    ).rejects.toThrow(/no contract/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when no tools are available', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('localFetch', false)],
        })),
      })
    ).rejects.toThrow(/No native tools are available/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime on an ABI mismatch', async () => {
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
    await expect(createNativeMcp({ env: {}, binding })).rejects.toThrow(/ABI/);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('fails closed on the previous ABI (an addon without per-request options)', async () => {
    const binding = {
      NativeRuntime: class extends FakeRuntime {
        readonly abiVersion = NATIVE_ABI_VERSION - 1;
        constructor() {
          super(() => ({
            fingerprint: getNativeContractFingerprint(),
            tools: [tool('localFetch', true)],
          }));
        }
      },
    };
    await expect(createNativeMcp({ env: {}, binding })).rejects.toThrow(
      `Native addon ABI ${NATIVE_ABI_VERSION - 1} does not match expected ${NATIVE_ABI_VERSION}`
    );
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when the catalog exposes no fingerprint', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: '',
          tools: [tool('localFetch', true)],
        })),
      })
    ).rejects.toThrow(/does not expose a contract fingerprint/i);
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
