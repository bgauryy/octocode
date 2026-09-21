import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import {
  DIRECT_TOOL_DEFINITIONS,
  getNativeContractFingerprint,
} from '@octocodeai/config/schema';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';

/**
 * A tool as reported by the native runtime catalog: runtime truth only —
 * names, availability, and the enforcement contract fingerprint. Everything
 * agent-facing (`title`/`description`/`inputSchema`/`annotations` for
 * `registerTool`, server instructions) is sourced from
 * `@octocodeai/octocode-core`; the native embed carries no presentation.
 */
export interface NativeCatalogTool {
  name: string;
  shortDescription?: string;
  available: boolean;
}

export interface NativeCatalog {
  fingerprint: string;
  tools: NativeCatalogTool[];
}

export interface NativeRuntime {
  readonly abiVersion: number;
  catalog(): NativeCatalog;
  executeMcp(requestId: string, tool: string, input: unknown): Promise<unknown>;
  cancel(requestId: string): boolean;
  close(): Promise<void>;
}

export interface NativeRuntimeOptions {
  surface: string;
  regexWorkerPath?: string | undefined;
  timeoutSecs?: number | undefined;
  env?: Record<string, string> | undefined;
}

export interface NativeRuntimeBinding {
  NativeRuntime: new (options?: NativeRuntimeOptions) => NativeRuntime;
}

export interface NativeMcp {
  server: McpServer;
  runtime: NativeRuntime;
  catalog: NativeCatalog;
  close(): Promise<void>;
}

export interface NativeMcpOptions {
  env?: NodeJS.ProcessEnv;
  binding?: NativeRuntimeBinding;
}

const require = createRequire(import.meta.url);

export function loadNativeBinding(
  env: NodeJS.ProcessEnv = process.env
): NativeRuntimeBinding {
  const bindingPath =
    env.OCTOCODE_NATIVE_BINDING ??
    require.resolve('@octocodeai/octocode-native/runtime');
  const binding = require(bindingPath) as Partial<NativeRuntimeBinding>;
  if (typeof binding.NativeRuntime !== 'function') {
    throw new Error('The candidate addon does not export NativeRuntime');
  }
  return binding as NativeRuntimeBinding;
}

/**
 * The MCP SDK's `registerTool` is generic over a single, statically-known
 * schema. Registration here iterates a heterogeneous definition list resolved at
 * runtime, so the config/callback shape is bridged through this narrow signature.
 * Contract agreement between the advertised schema and the runtime that executes
 * it is guaranteed separately by the fingerprint check below — not by this type.
 */
type RegisterTool = (
  name: string,
  config: {
    title?: string;
    description?: string;
    inputSchema?: unknown;
    annotations?: unknown;
  },
  callback: (
    args: unknown,
    context?: { signal?: AbortSignal; requestId?: string }
  ) => Promise<unknown>
) => void;

export function createNativeMcp({
  env = process.env,
  binding,
}: NativeMcpOptions = {}): NativeMcp {
  const { NativeRuntime } = binding ?? loadNativeBinding(env);
  const runtimeEnv = Object.fromEntries(
    Object.entries(env).filter(
      (entry): entry is [string, string] => typeof entry[1] === 'string'
    )
  );
  const runtime = new NativeRuntime({
    surface: 'mcp',
    regexWorkerPath: env.OCTOCODE_REGEX_WORKER,
    env: runtimeEnv,
    // Match the CLI budget and exceed cold start plus one logical LSP request:
    // initialize, Java readiness, retries, delays, and transport overhead.
    timeoutSecs: 300,
  });
  if (runtime.abiVersion !== NATIVE_ABI_VERSION) {
    const actual = runtime.abiVersion;
    void runtime.close();
    throw new Error(
      `Native addon ABI ${actual} does not match expected ${NATIVE_ABI_VERSION}; ` +
        'rebuild or reinstall @octocodeai/octocode-native'
    );
  }
  const catalog = runtime.catalog();
  const availableTools = catalog.tools.filter(tool => tool.available);
  if (availableTools.length === 0) {
    void runtime.close();
    throw new Error('No native tools are available');
  }

  // Validate the contract identity before constructing anything else. The
  // native runtime and core package independently embed the same canonical
  // contract IR; compare that shared identity rather than unlike runtime
  // representations (native JSON Schema versus Standard Schema/Zod objects).
  const coreFingerprint = getNativeContractFingerprint();
  const nativeFingerprint = catalog.fingerprint;
  if (typeof nativeFingerprint !== 'string' || nativeFingerprint.length === 0) {
    void runtime.close();
    throw new Error('Native catalog does not expose a contract fingerprint');
  }
  if (nativeFingerprint !== coreFingerprint) {
    // Fail closed. Registration advertises core's Zod schemas while the native
    // runtime validates against its own embedded JSON-Schema contract. When the
    // two drift, clients are shown a schema the runtime rejects — so refuse to
    // start rather than serve a mismatched contract. Set
    // OCTOCODE_ALLOW_CONTRACT_DRIFT=1 to downgrade to a warning while iterating
    // on core and native locally.
    const message =
      '[octocode-mcp] contract fingerprint mismatch: core ' +
      `${coreFingerprint} (@octocodeai/octocode-core) != native ` +
      `${nativeFingerprint}. Realign the core package and the native generated ` +
      'contract, or set OCTOCODE_ALLOW_CONTRACT_DRIFT=1 to override.';
    if (env.OCTOCODE_ALLOW_CONTRACT_DRIFT === '1') {
      // stderr, not stdout: stdout is reserved for the MCP stdio protocol.
      process.stderr.write(`WARNING (override active): ${message}\n`);
    } else {
      void runtime.close();
      throw new Error(message);
    }
  }

  // Server identity is interface-owned: the native contract carries tool
  // guidance, not the MCP server's name/title/version.
  const implementation = {
    name: 'octocode-mcp_native-candidate',
    title: 'Octocode MCP',
    version: '0.1.0',
  };
  const server = new McpServer(implementation, {
    capabilities: { tools: { listChanged: false } },
    // Availability-scoped instructions, built by core from the tools the
    // native runtime actually enables — the native catalog carries none.
    instructions: buildMcpInstructions(availableTools.map(tool => tool.name)),
  });
  const registerTool = server.registerTool.bind(server) as RegisterTool;

  const definitions = new Map(
    DIRECT_TOOL_DEFINITIONS.map(definition => [definition.name, definition])
  );

  for (const tool of availableTools) {
    const definition = definitions.get(tool.name);
    if (!definition) {
      void runtime.close();
      throw new Error(`Native catalog tool has no contract: ${tool.name}`);
    }
    registerTool(
      tool.name,
      {
        title: definition.title,
        description: definition.description,
        inputSchema: definition.inputSchema,
        annotations: definition.annotations,
      },
      async (args, context = {}) => {
        const signal = context.signal;
        const requestId = String(context.requestId ?? randomUUID());
        signal?.throwIfAborted();
        const cancel = () => runtime.cancel(requestId);
        signal?.addEventListener('abort', cancel, { once: true });
        try {
          return await runtime.executeMcp(requestId, tool.name, args);
        } finally {
          signal?.removeEventListener('abort', cancel);
        }
      }
    );
  }

  let closing: Promise<void> | undefined;
  const close = (): Promise<void> =>
    (closing ??= (async () => {
      await runtime.close();
      await server.close();
    })());
  return { server, runtime, catalog, close };
}

export async function startNativeMcp(
  options?: NativeMcpOptions
): Promise<NativeMcp> {
  const instance = createNativeMcp(options);
  // Drain in-flight requests (runtime.close awaits active_requests==0) and close
  // the server before exiting, rather than fire-and-forget, so shutdown does not
  // truncate a request mid-flight.
  const shutdown = (): void => {
    void instance.close().finally(() => process.exit(0));
  };
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
  process.stdin.once('end', shutdown);
  await instance.server.connect(new StdioServerTransport());
  return instance;
}
