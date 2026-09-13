import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { Server } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';

const require = createRequire(import.meta.url);

export function loadNativeBinding(env = process.env) {
  const bindingPath = env.OCTOCODE_NATIVE_BINDING;
  if (!bindingPath) {
    throw new Error('OCTOCODE_NATIVE_BINDING must identify the candidate addon');
  }
  const binding = require(bindingPath);
  if (typeof binding.NativeRuntime !== 'function') {
    throw new Error('The candidate addon does not export NativeRuntime');
  }
  return binding;
}

function toolDescriptor(tool) {
  const descriptor = {
    name: tool.name,
    title: tool.title,
    description: tool.description,
    inputSchema: tool.inputSchema,
  };
  if (tool.outputSchema) descriptor.outputSchema = tool.outputSchema;
  if (tool.annotations) descriptor.annotations = tool.annotations;
  return descriptor;
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
  const server = new Server(implementation, {
    capabilities: { tools: { listChanged: false } },
    instructions: availableTools.length === 1 && availableTools[0].name === 'localFetch'
      ? catalog.mcpInstructionsByEnabledSet?.localFetch ?? catalog.mcpInstructions
      : catalog.mcpInstructions,
  });

  server.setRequestHandler('tools/list', () => ({
    tools: availableTools.map(toolDescriptor),
  }));

  server.setRequestHandler('tools/call', async (request, context = {}) => {
    const signal = context.signal ?? context.mcpReq?.signal;
    const requestId = String(context.requestId ?? context.mcpReq?.id ?? randomUUID());
    const cancel = () => runtime.cancel(requestId);
    signal?.addEventListener('abort', cancel, { once: true });
    try {
      if (signal?.aborted) runtime.cancel(requestId);
      return await runtime.executeMcp(
        requestId,
        request.params.name,
        request.params.arguments ?? {},
      );
    } finally {
      signal?.removeEventListener('abort', cancel);
    }
  });

  let closing;
  const close = () => closing ??= (async () => {
    await runtime.close();
    await server.close();
  })();
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

const invokedPath = process.argv[1] && pathToFileURL(process.argv[1]).href;
if (invokedPath === import.meta.url) await startNativeMcp();
