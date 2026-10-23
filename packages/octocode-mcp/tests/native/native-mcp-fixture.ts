// Shared fake native runtime for the createNativeMcp/startNativeMcp suites:
// exercises the src-level registration/execution path through a real McpServer
// + in-memory client without loading the native addon.
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import type {
  NativeCatalog,
  NativeCatalogTool,
  NativeMcp,
  NativeRuntime,
  NativeRuntimeOptions,
} from '../../src/native/index.js';

interface RecordedExecution {
  requestId: string;
  tool: string;
  input: unknown;
}

// The fake speaks the ABI the MCP expects; src/native/index.ts already imports
// this module, so importing it here loads nothing extra.
const FAKE_ABI_VERSION = NATIVE_ABI_VERSION;

/** A native `CallToolResult`: one text block, its structuredContent's JSON. */
export function nativeResult(structuredContent: unknown, isError: boolean) {
  return {
    content: [{ type: 'text', text: JSON.stringify(structuredContent) }],
    structuredContent,
    isError,
  };
}

export class FakeRuntime implements NativeRuntime {
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
        return nativeResult(
          {
            error: 'Classification provider returned HTTP 401',
            errorCode: 'classificationProviderError',
          },
          true
        );
      }
      return nativeResult({ queries: [{ id: 'q1', status: 'ok' }] }, false);
    }
    return nativeResult({ results: [{ index: 0, data: { ok: true } }] }, false);
  }

  async close(): Promise<void> {
    this.closed = true;
    this.closeCount += 1;
  }
}

export const bindingFor = (makeCatalog: () => NativeCatalog) => ({
  NativeRuntime: class extends FakeRuntime {
    constructor(options?: NativeRuntimeOptions) {
      super(makeCatalog, options);
    }
  },
});

export const tool = (
  name: string,
  available: boolean,
  extra: Partial<NativeCatalogTool> = {}
): NativeCatalogTool => ({
  name,
  shortDescription: `${name} description.`,
  available,
  ...extra,
});

/** An MCP client connected to a fresh server from the instance's factory. */
export async function connectClient(
  instance: NativeMcp,
  name: string
): Promise<Client> {
  const client = new Client({ name, version: '1' });
  const [serverTransport, clientTransport] =
    InMemoryTransport.createLinkedPair();
  await Promise.all([
    instance.createServer().connect(serverTransport),
    client.connect(clientTransport),
  ]);
  return client;
}
