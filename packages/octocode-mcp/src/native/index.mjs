import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import {
  DIRECT_TOOL_DEFINITIONS,
  getNativeContractFingerprint,
} from '@octocodeai/octocode-core/schema';

const require = createRequire(import.meta.url);

// Must match `NATIVE_ABI_VERSION` in crates/runtime/src/lib.rs. The loader
// fails closed on mismatch so a stale/ABI-incompatible addon (e.g. via the
// OCTOCODE_NATIVE_BINDING override or independent npm resolution) surfaces a
// precise error instead of an obscure struct/method-layout crash.
const EXPECTED_ABI_VERSION = 2;

export function loadNativeBinding(env = process.env) {
  const bindingPath =
    env.OCTOCODE_NATIVE_BINDING ??
    require.resolve('@octocodeai/octocode-native/runtime');
  const binding = require(bindingPath);
  if (typeof binding.NativeRuntime !== 'function') {
    throw new Error('The candidate addon does not export NativeRuntime');
  }
  return binding;
}

export function createNativeMcp({ env = process.env, binding } = {}) {
  const { NativeRuntime } = binding ?? loadNativeBinding(env);
  const runtime = new NativeRuntime({
    surface: 'mcp',
    regexWorkerPath: env.OCTOCODE_REGEX_WORKER,
  });
  if (runtime.abiVersion !== EXPECTED_ABI_VERSION) {
    const actual = runtime.abiVersion;
    void runtime.close();
    throw new Error(
      `Native addon ABI ${actual} does not match expected ${EXPECTED_ABI_VERSION}; ` +
        'rebuild or reinstall @octocodeai/octocode-native'
    );
  }
  const catalog = runtime.catalog();
  const availableTools = catalog.tools.filter(tool => tool.available);
  if (availableTools.length === 0) {
    void runtime.close();
    throw new Error('No native tools are available');
  }
  const implementation = catalog.server ?? {
    name: 'octocode-mcp_native-candidate',
    title: 'Octocode MCP',
    version: '0.1.0',
  };
  const server = new McpServer(implementation, {
    capabilities: { tools: { listChanged: false } },
    instructions: catalog.mcpInstructions,
  });

  const definitions = new Map(
    DIRECT_TOOL_DEFINITIONS.map(definition => [definition.name, definition])
  );

  // The native runtime and core package independently embed the same canonical
  // contract IR. Compare that shared identity rather than unlike runtime
  // representations (native JSON Schema versus Standard Schema/Zod objects).
  const coreFingerprint = getNativeContractFingerprint();
  const nativeFingerprint = catalog.fingerprint;
  if (typeof nativeFingerprint !== 'string' || nativeFingerprint.length === 0) {
    void runtime.close();
    throw new Error('Native catalog does not expose a contract fingerprint');
  }
  if (nativeFingerprint !== coreFingerprint) {
    console.error(
      '[octocode-mcp] WARNING: advertised contract fingerprint ' +
        `${coreFingerprint} (@octocodeai/octocode-core) does not match native ` +
        `${nativeFingerprint}. Clients may be shown a contract the runtime rejects; ` +
        'realign the core package and native generated contract.'
    );
  }

  for (const tool of availableTools) {
    const definition = definitions.get(tool.name);
    if (!definition) {
      void runtime.close();
      throw new Error(`Native catalog tool has no contract: ${tool.name}`);
    }
    server.registerTool(
      tool.name,
      {
        title: definition.title,
        description: definition.description,
        inputSchema: definition.inputSchema,
        outputSchema: definition.outputSchema,
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

  let closing;
  const close = () =>
    (closing ??= (async () => {
      await runtime.close();
      await server.close();
    })());
  return { server, runtime, catalog, close };
}

export async function startNativeMcp(options) {
  const instance = createNativeMcp(options);
  // Drain in-flight requests (runtime.close awaits active_requests==0) and close
  // the server before exiting, rather than fire-and-forget, so shutdown does not
  // truncate a request mid-flight.
  const shutdown = () => {
    instance.close().finally(() => process.exit(0));
  };
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
  process.stdin.once('end', shutdown);
  await instance.server.connect(new StdioServerTransport());
  return instance;
}

// Only self-start when this module is the process entry point AND was invoked
// directly as the native entry file. The basename guard prevents misfiring when
// the module is bundled into or imported by another entry (e.g. index.js).
const entryArg = process.argv[1] ?? '';
const invokedPath = entryArg ? pathToFileURL(entryArg).href : '';
if (
  invokedPath === import.meta.url &&
  /native[\\/]index\.mjs$/.test(entryArg)
) {
  await startNativeMcp();
}
