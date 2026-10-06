import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { z } from 'zod';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import {
  contractDriftAllowed,
  contractDriftMessage,
  devOverridesAllowed,
  INTERACTIVE_EXECUTION_TIMEOUT_SECS,
  setRuntimeSurface,
  type RuntimeSurface,
} from '@octocodeai/config';
import {
  getDirectToolDefinitionsWithAddons,
  getNativeContractFingerprint,
  isCliOnlyTool,
} from '@octocodeai/config/schema';
import {
  buildMcpInstructions,
  DEFERRED_TOOL_DISPATCHER,
  deferredDispatcherDefinition,
  publishedInputSchema,
  type GrammarCapability,
} from '@octocodeai/config/mcp';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import packageJson from '../../package.json';

/**
 * A tool as reported by the native runtime catalog: runtime truth only —
 * names, availability, and the enforcement contract fingerprint. Everything
 * agent-facing (`title`/`description`/`inputSchema` for `registerTool`,
 * server instructions) is composed by the `@octocodeai/config` contract hub
 * from canonical core contracts plus capability-aware addons; the native embed
 * carries no presentation.
 */
export interface NativeCatalogTool {
  name: string;
  shortDescription?: string;
  available: boolean;
}

export interface NativeCatalog {
  fingerprint: string;
  tools: NativeCatalogTool[];
  grammarCapabilities?: GrammarCapability[];
  /** Language labels whose LSP server resolves on this machine. */
  lspServers?: string[];
  /**
   * MCP presentation switches resolved by native from config `mcp.*`: the
   * available tools served only through the deferred-tool dispatcher.
   */
  presentation?: {
    deferred?: string[];
  };
}

/** Native startup check of the clasify provider (`probeClassification`). */
export interface ClassificationProbe {
  probed: boolean;
  available: boolean;
  code?: string;
  message?: string;
}

export interface NativeRuntime {
  readonly abiVersion: number;
  /** Disables clasify in `catalog()` when its provider cannot answer. */
  probeClassification(): Promise<ClassificationProbe>;
  catalog(): NativeCatalog;
  executeMcp(requestId: string, tool: string, input: unknown): Promise<unknown>;
  cancel(requestId: string): boolean;
  close(): Promise<void>;
}

export interface NativeRuntimeOptions {
  surface: RuntimeSurface;
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

const MCP_SURFACE = 'mcp' satisfies RuntimeSurface;

// Replaced with `true` by the esbuild define in buildConfig.mjs; undefined when
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

/**
 * The MCP SDK's `registerTool` is generic over a single, statically-known
 * schema. Registration here iterates a heterogeneous definition list resolved at
 * runtime, so the config/callback shape is bridged through this narrow signature.
 * Contract agreement between the advertised schema and the runtime that executes
 * it is guaranteed separately by the fingerprint check below — not by this type.
 */
// @modelcontextprotocol/server 2.x exposes the per-request abort signal and
// JSON-RPC id under `ctx.mcpReq`; the flat `signal` / `requestId` shape is the
// 1.x layout, kept only as a harmless fallback.
type ToolCallContext = {
  mcpReq?: { signal?: AbortSignal; id?: string | number };
  signal?: AbortSignal;
  requestId?: string | number;
};

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

type StandardSchema = {
  '~standard': {
    version: 1;
    vendor: string;
    validate: (value: unknown) => { value: unknown };
    jsonSchema: {
      input: () => Record<string, unknown>;
      output: () => Record<string, unknown>;
    };
  };
};

type ToolDefinition = ReturnType<
  typeof getDirectToolDefinitionsWithAddons
>[number];

/**
 * A Standard Schema that advertises `jsonSchema` in tools/list and passes
 * every input through: native normalizes and validates once, isolates invalid
 * rows, and returns the actionable repair message for a rejected call.
 */
function passThroughSchema(
  advertised: Record<string, unknown>
): StandardSchema {
  return {
    '~standard': {
      version: 1,
      vendor: 'octocode',
      jsonSchema: { input: () => advertised, output: () => advertised },
      validate: value => ({ value }),
    },
  };
}

/** A tool's registered schema: core's slim published view of the canonical input. */
export function toolInputSchema(
  definition: Pick<ToolDefinition, 'name' | 'inputSchema'>
): StandardSchema {
  return passThroughSchema(
    publishedInputSchema(definition.name, canonicalInputSchema(definition))
  );
}

function canonicalInputSchema(
  definition: Pick<ToolDefinition, 'inputSchema'>
): Record<string, unknown> {
  return z.toJSONSchema(definition.inputSchema, {
    io: 'input',
    unrepresentable: 'any',
  }) as Record<string, unknown>;
}

/**
 * MCP clients that read only `content` must still see the result. When the
 * native envelope carries structuredContent but no text block (clasify
 * receipts are not text-rendered), serialize structuredContent as the text
 * block, as the MCP spec recommends for structured results.
 */
function ensureTextContent(result: unknown): unknown {
  if (!result || typeof result !== 'object' || Array.isArray(result)) {
    return result;
  }
  const envelope = result as {
    content?: unknown;
    structuredContent?: unknown;
  };
  const content = Array.isArray(envelope.content) ? envelope.content : [];
  if (content.length > 0 || envelope.structuredContent === undefined) {
    return result;
  }
  return {
    ...envelope,
    content: [
      { type: 'text', text: JSON.stringify(envelope.structuredContent) },
    ],
  };
}

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
  setRuntimeSurface(MCP_SURFACE);
  const runtime = new NativeRuntime({
    surface: MCP_SURFACE,
    regexWorkerPath: env.OCTOCODE_REGEX_WORKER,
    env: runtimeEnv,
    timeoutSecs: INTERACTIVE_EXECUTION_TIMEOUT_SECS,
  });
  if (runtime.abiVersion !== NATIVE_ABI_VERSION) {
    const actual = runtime.abiVersion;
    void runtime.close();
    throw new Error(
      `Native addon ABI ${actual} does not match expected ${NATIVE_ABI_VERSION}; ` +
        'rebuild or reinstall @octocodeai/octocode-native'
    );
  }
  // An enabled clasify whose provider cannot answer (bad key, quota, host
  // down) leaves the catalog before tools/list, so agents never call it.
  const probe = await runtime.probeClassification();
  if (probe.probed && !probe.available) {
    process.stderr.write(
      `[octocode-mcp] clasify disabled: provider check failed (${probe.code}): ${probe.message}\n`
    );
  }
  const catalog = runtime.catalog();
  // Second guard: CLI-only tools (core tool policy) must never be exposed
  // over MCP, even if the native catalog reports them available.
  const availableTools = catalog.tools.filter(
    tool => tool.available && !isCliOnlyTool(tool.name)
  );
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
  // Presentation switches (config `mcp.*`, resolved by native). Deferred
  // tools stay available but leave tools/list; `run` executes them.
  const presentation = catalog.presentation ?? {};
  const availableNames = availableTools.map(tool => tool.name);
  const deferred = availableNames.filter(name =>
    presentation.deferred?.includes(name)
  );
  const listed = availableTools.filter(tool => !deferred.includes(tool.name));
  const server = new McpServer(implementation, {
    capabilities: { tools: { listChanged: false } },
    // Availability-scoped instructions, built by core from the tools the
    // native runtime actually enables — the native catalog carries none.
    // Hosts truncate instructions near 2 KB, so the grammar inventory stays
    // with `octocode scheme` rather than being appended here.
    instructions: buildMcpInstructions(
      listed.map(tool => tool.name),
      { deferred }
    ),
  });
  const registerTool = server.registerTool.bind(server) as RegisterTool;

  const definitions = new Map(
    getDirectToolDefinitionsWithAddons({
      availableTools: availableNames,
      ...(catalog.lspServers && { lspServers: catalog.lspServers }),
    }).map(definition => [definition.name, definition])
  );

  const execute = async (
    toolName: string,
    args: unknown,
    context: ToolCallContext
  ): Promise<unknown> => {
    const signal = context.mcpReq?.signal ?? context.signal;
    const requestId = String(
      context.mcpReq?.id ?? context.requestId ?? randomUUID()
    );
    signal?.throwIfAborted();
    const cancel = () => runtime.cancel(requestId);
    signal?.addEventListener('abort', cancel, { once: true });
    try {
      return ensureTextContent(
        await runtime.executeMcp(requestId, toolName, args)
      );
    } catch (error) {
      // A thrown rejection here is an internal/native failure (not a normal
      // tool error, which is returned in the result envelope). The SDK would
      // surface its raw message verbatim to the client, so backstop it:
      // cancellations propagate unchanged; everything else is logged to
      // stderr and replaced with a generic client-facing message so paths,
      // ids, or token fragments in native error text never leak.
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
    const definition = definitions.get(tool.name);
    if (!definition) {
      void runtime.close();
      throw new Error(`Native catalog tool has no contract: ${tool.name}`);
    }
    if (deferred.includes(tool.name)) continue;
    registerTool(
      tool.name,
      {
        title: definition.title,
        description: definition.description,
        inputSchema: toolInputSchema(definition),
        // Core-authored MCP hints (readOnlyHint), passed through as-is.
        ...(definition.annotations && { annotations: definition.annotations }),
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
        canonical: canonicalInputSchema(definitions.get(name)!),
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
        if (typeof tool !== 'string' || !availableNames.includes(tool)) {
          return {
            content: [
              {
                type: 'text',
                text: `Input validation error: Invalid arguments for tool ${DEFERRED_TOOL_DISPATCHER}: tool: ${String(tool)} is not available; use one of ${availableNames.join(', ')}`,
              },
            ],
            isError: true,
          };
        }
        // Verbatim: native runs a bare row as a one-row `queries`.
        return execute(tool, query, context);
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
  const instance = await createNativeMcp(options);
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
    void instance.close().finally(() => {
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
    void instance.close().finally(() => {
      clearTimeout(forceExit);
      process.exit(1);
    });
  };
  process.once('uncaughtException', fatal('uncaughtException'));
  process.once('unhandledRejection', fatal('unhandledRejection'));
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
  process.stdin.once('end', shutdown);
  await instance.server.connect(new StdioServerTransport());
  return instance;
}
