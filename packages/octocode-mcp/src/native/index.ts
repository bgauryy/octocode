import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { McpServer } from '@modelcontextprotocol/server';
import { serveStdio } from '@modelcontextprotocol/server/stdio';
import {
  contractDriftAllowed,
  contractDriftMessage,
  devOverridesAllowed,
  type RuntimeSurface,
} from '@octocodeai/config';
import {
  getDirectToolDefinitionsWithAddons,
  getNativeContractFingerprint,
} from '@octocodeai/config/schema';
import {
  buildMcpInstructions,
  deferredDispatcherDefinition,
} from '@octocodeai/config/mcp';
import {
  NATIVE_ABI_VERSION,
  type NativeCatalog,
  type NativeCatalogTool,
  type NativeRuntime as NativeAddonRuntime,
  type NativeRuntimeOptions,
} from '@octocodeai/octocode-native/runtime';
import packageJson from '../../package.json';
import {
  passThroughSchema,
  toolInputSchema,
  type ToolDefinition,
} from './schema.js';

export { toolInputSchema } from './schema.js';

/**
 * The native runtime surface this adapter uses, declared once by
 * `@octocodeai/octocode-native`: the catalog carries runtime truth only —
 * names, availability, and the enforcement contract fingerprint. Everything
 * agent-facing (`title`/`description`/`inputSchema` for `registerTool`,
 * server instructions) is composed by the `@octocodeai/config` contract hub
 * from canonical core contracts plus capability-aware addons.
 */
export type NativeRuntime = Pick<
  NativeAddonRuntime,
  'abiVersion' | 'catalog' | 'executeMcp' | 'cancel' | 'close'
>;

export type { NativeCatalog, NativeCatalogTool, NativeRuntimeOptions };

export interface NativeRuntimeBinding {
  NativeRuntime: new (options?: NativeRuntimeOptions) => NativeRuntime;
}

export interface NativeMcp {
  /** A new MCP server over the shared runtime: one per connection era. */
  createServer(): McpServer;
  runtime: NativeRuntime;
  catalog: NativeCatalog;
  close(): Promise<void>;
}

export interface NativeMcpOptions {
  env?: NodeJS.ProcessEnv;
  binding?: NativeRuntimeBinding;
}

const require = createRequire(import.meta.url);

const MCP_SURFACE = 'mcp' satisfies RuntimeSurface;

// Replaced with `true` by the esbuild define in build.mjs; undefined when
// running from source (vitest, tsx).
declare const __OCTOCODE_BUNDLED__: boolean | undefined;

/** Dev overrides follow the shared config rule; the shipped bundle is production. */
const overrideOptions = {
  bundled:
    typeof __OCTOCODE_BUNDLED__ !== 'undefined' &&
    __OCTOCODE_BUNDLED__ === true,
};

export function loadNativeBinding(
  env: NodeJS.ProcessEnv = process.env
): NativeRuntimeBinding {
  // `OCTOCODE_NATIVE_BINDING` require()s an arbitrary path (candidate-addon dev
  // aid). Honor it only outside production so a leaked/hostile env value cannot
  // load arbitrary code into a shipped server; production always resolves the
  // packaged addon.
  const override = devOverridesAllowed(env, overrideOptions)
    ? env.OCTOCODE_NATIVE_BINDING
    : undefined;
  const bindingPath =
    override ?? require.resolve('@octocodeai/octocode-native/runtime');
  const binding = require(bindingPath) as Partial<NativeRuntimeBinding>;
  if (typeof binding.NativeRuntime !== 'function') {
    throw new Error('The candidate addon does not export NativeRuntime');
  }
  return binding as NativeRuntimeBinding;
}

// @modelcontextprotocol/server 2.x carries the per-request abort signal and
// JSON-RPC id under `ctx.mcpReq`.
type ToolCallContext = {
  mcpReq?: { signal?: AbortSignal; id?: string | number };
};

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
    annotations?: ToolDefinition['annotations'];
  },
  callback: (args: unknown, context?: ToolCallContext) => Promise<unknown>
) => void;

export async function createNativeMcp({
  env = process.env,
  binding,
}: NativeMcpOptions = {}): Promise<NativeMcp> {
  const { NativeRuntime } = binding ?? loadNativeBinding(env);
  const runtimeEnv = Object.fromEntries(
    Object.entries(env).filter(
      (entry): entry is [string, string] => typeof entry[1] === 'string'
    )
  );
  // Native owns the per-request execution budget of every surface.
  const runtime = new NativeRuntime({ surface: MCP_SURFACE, env: runtimeEnv });
  if (runtime.abiVersion !== NATIVE_ABI_VERSION) {
    const actual = runtime.abiVersion;
    void runtime.close();
    throw new Error(
      `Native addon ABI ${actual} does not match expected ${NATIVE_ABI_VERSION}; ` +
        'rebuild or reinstall @octocodeai/octocode-native'
    );
  }
  // Native resolves availability for the MCP surface (CLI-only tools are
  // never available here) and refuses CLI-only tools at MCP admission.
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
    const message = `[octocode-mcp] ${contractDriftMessage(coreFingerprint, nativeFingerprint)}`;
    if (contractDriftAllowed(env, overrideOptions)) {
      // stderr, not stdout: stdout is reserved for the MCP stdio protocol.
      // The override is a local-iteration aid only; in production a fingerprint
      // mismatch always fails closed so clients never see a rejected contract.
      process.stderr.write(`WARNING (override active): ${message}\n`);
    } else {
      void runtime.close();
      throw new Error(message);
    }
  }

  // Server identity is interface-owned: the native contract carries tool
  // guidance, not the MCP server's name/title/version.
  const implementation = {
    name: 'octocode-mcp',
    title: 'Octocode MCP',
    version: packageJson.version,
  };
  // Presentation switches resolved by native. Deferred
  // tools stay available but leave tools/list; `run` executes them.
  const presentation = catalog.presentation ?? {};
  const availableNames = availableTools.map(tool => tool.name);
  const deferred = availableNames.filter(name =>
    presentation.deferred?.includes(name)
  );
  // Core owns the prose; native supplies the same capabilities used by schemas.
  const instructions = buildMcpInstructions({
    availableTools: availableNames,
    selection: presentation.selection === true,
  });

  const definitions = new Map(
    getDirectToolDefinitionsWithAddons({
      availableTools: availableNames,
      selection: presentation.selection === true,
      ...(catalog.lspServers && { lspServers: catalog.lspServers }),
    }).map(definition => [definition.name, definition])
  );

  const execute = async (
    toolName: string,
    args: unknown,
    context: ToolCallContext
  ): Promise<unknown> => {
    const signal = context.mcpReq?.signal;
    const requestId = randomUUID();
    signal?.throwIfAborted();
    const cancel = () => runtime.cancel(requestId);
    signal?.addEventListener('abort', cancel, { once: true });
    try {
      // Native answers every rejected call as a typed tool error; it rejects
      // only a cancelled request or a closing runtime. Its one text block is
      // the serialized JSON of its structuredContent.
      return await runtime.executeMcp(requestId, toolName, args);
    } catch (error) {
      // Cancellations propagate unchanged. Anything else (shutdown, a missing
      // or broken addon) is logged to stderr; the client gets a generic
      // message so raw error text never leaks.
      if (signal?.aborted) throw error;
      const detail = error instanceof Error ? error.message : String(error);
      process.stderr.write(
        `[octocode-mcp] ${toolName} execution error: ${detail}\n`
      );
      throw new Error(
        `Tool ${toolName} failed to execute; see the server logs for detail.`,
        { cause: error }
      );
    } finally {
      signal?.removeEventListener('abort', cancel);
    }
  };

  for (const tool of availableTools) {
    if (!definitions.has(tool.name)) {
      void runtime.close();
      throw new Error(`Native catalog tool has no contract: ${tool.name}`);
    }
  }

  const servers = new Set<McpServer>();
  const createServer = (): McpServer => {
    const server = new McpServer(implementation, {
      capabilities: { tools: { listChanged: false } },
      instructions,
    });
    servers.add(server);
    register(server.registerTool.bind(server) as RegisterTool);
    return server;
  };

  const register = (registerTool: RegisterTool): void => {
    for (const tool of availableTools) {
      const definition = definitions.get(tool.name)!;
      if (deferred.includes(tool.name)) continue;
      registerTool(
        tool.name,
        {
          title: definition.title,
          description: definition.description,
          inputSchema: toolInputSchema(definition),
          // Core-authored MCP hints (readOnlyHint), passed through as-is.
          ...(definition.annotations && {
            annotations: definition.annotations,
          }),
        },
        (args, context = {}) => execute(tool.name, args, context)
      );
    }

    if (deferred.length) {
      // Every next/hints lead is `{tool, query}`, so a lead naming a deferred
      // tool runs here verbatim; native validates its query exactly as a
      // direct call.
      const dispatcher = deferredDispatcherDefinition(
        deferred.map(name => ({
          name,
          inputSchema: definitions.get(name)!.inputSchema,
        }))
      );
      registerTool(
        dispatcher.name,
        {
          title: dispatcher.title,
          description: dispatcher.description,
          inputSchema: passThroughSchema(dispatcher.inputSchema),
        },
        async (args, context = {}) => {
          const { tool, query } = (args ?? {}) as {
            tool?: unknown;
            query?: unknown;
          };
          // Verbatim: native runs a bare row as a one-row `queries`, and
          // answers an unknown or unavailable tool as a typed tool error.
          return execute(String(tool), query, context);
        }
      );
    }
  };

  let closing: Promise<void> | undefined;
  const close = (): Promise<void> =>
    (closing ??= (async () => {
      await runtime.close();
      await Promise.all([...servers].map(server => server.close()));
    })());
  return { createServer, runtime, catalog, close };
}

export async function startNativeMcp(
  options?: NativeMcpOptions
): Promise<NativeMcp> {
  const instance = await createNativeMcp(options);
  // One stdio connection serves either era: a 2025-era client opens with
  // `initialize`, a 2026-07-28 client with `server/discover`; the SDK pins one
  // server from the factory to the connection.
  const connection = serveStdio(instance.createServer, {
    onerror: error =>
      process.stderr.write(`[octocode-mcp] transport: ${error.message}\n`),
  });
  const closeAll = (): Promise<void> =>
    connection.close().finally(() => instance.close());
  // Shutdown = cancel + bounded wait. `runtime.close()` (native `begin_close`)
  // cancels every in-flight request immediately and then waits for the active
  // requests to unwind; it does NOT let them finish. A request in flight when
  // stdin closes therefore ends as `cancelled` and its client gets no reply.
  // The wait is bounded so a stuck request cannot hang the process past an
  // orchestrator's grace window (which then SIGKILLs anyway); whichever of
  // {close complete, grace elapsed} comes first exits cleanly.
  const SHUTDOWN_GRACE_MS = 10_000;
  const shutdown = (): void => {
    const forceExit = setTimeout(() => process.exit(0), SHUTDOWN_GRACE_MS);
    forceExit.unref?.();
    void closeAll().finally(() => {
      clearTimeout(forceExit);
      process.exit(0);
    });
  };
  // Last-resort fatal handlers. Log to stderr only (stdout carries MCP
  // JSON-RPC), then close the runtime and exit non-zero.
  let fatalInProgress = false;
  const fatal = (kind: string) => (reason: unknown) => {
    const detail =
      reason instanceof Error
        ? (reason.stack ?? reason.message)
        : String(reason);
    process.stderr.write(`[octocode-mcp] ${kind}: ${detail}\n`);
    if (fatalInProgress) return;
    fatalInProgress = true;
    const forceExit = setTimeout(() => process.exit(1), SHUTDOWN_GRACE_MS);
    forceExit.unref?.();
    void closeAll().finally(() => {
      clearTimeout(forceExit);
      process.exit(1);
    });
  };
  process.once('uncaughtException', fatal('uncaughtException'));
  process.once('unhandledRejection', fatal('unhandledRejection'));
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
  process.stdin.once('end', shutdown);
  return instance;
}
