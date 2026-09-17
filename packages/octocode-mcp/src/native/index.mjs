import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/octocode-core/schema';
import { buildMcpInstructions } from '@octocodeai/octocode-core/mcp';
import { getGrammarCapabilities } from '@octocodeai/octocode-tools-core';

const require = createRequire(import.meta.url);

export function loadNativeBinding(env = process.env) {
  const bindingPath =
    env.OCTOCODE_NATIVE_BINDING ??
    require.resolve('@octocodeai/octocode-native/native.cjs');
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
    instructions: buildMcpInstructions(
      availableTools.map(tool => tool.name),
      availableTools.some(tool => tool.name === 'astSearch')
        ? { grammarCapabilities: getGrammarCapabilities() }
        : {}
    ),
  });

  const definitions = new Map(
    DIRECT_TOOL_DEFINITIONS.map(definition => [definition.name, definition])
  );
  for (const tool of availableTools) {
    const definition = definitions.get(tool.name);
    if (!definition) {
      void runtime.close();
      throw new Error(
        `Native catalog tool has no octocode-core definition: ${tool.name}`
      );
    }
    server.registerTool(
      definition.name,
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
          return await runtime.executeMcp(requestId, definition.name, args);
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
  process.once('SIGINT', () => void instance.close());
  process.once('SIGTERM', () => void instance.close());
  process.stdin.once('end', () => void instance.close());
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
