import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { z } from 'zod';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import {
  getDirectToolDefinitionsWithAddons,
  getNativeContractFingerprint,
} from '@octocodeai/config/schema';
import {
  buildMcpInstructions,
  type GrammarCapability,
} from '@octocodeai/config/mcp';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import packageJson from '../../package.json';
import { formatIssues, type RawIssue } from './validationMessages.js';

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
  // `OCTOCODE_NATIVE_BINDING` require()s an arbitrary path (candidate-addon dev
  // aid). Honor it only outside production so a leaked/hostile env value cannot
  // load arbitrary code into a shipped server; production always resolves the
  // packaged addon.
  const override =
    env.NODE_ENV === 'production' ? undefined : env.OCTOCODE_NATIVE_BINDING;
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
type RegisterTool = (
  name: string,
  config: {
    title?: string;
    description?: string;
    inputSchema?: unknown;
  },
  callback: (
    args: unknown,
    context?: { signal?: AbortSignal; requestId?: string }
  ) => Promise<unknown>
) => void;

type StandardResult =
  { value: unknown; issues?: undefined } | { issues: readonly unknown[] };
type StandardSchema = {
  '~standard': {
    version: 1;
    vendor: string;
    validate: (value: unknown) => StandardResult | Promise<StandardResult>;
    jsonSchema?: unknown;
  };
};

// CLI and the native runtime accept a bare single query (the envelope rule
// wraps it); normalize the same convenience before SDK validation.
export function wrapBareQuery(input: unknown): unknown {
  return input &&
    typeof input === 'object' &&
    !Array.isArray(input) &&
    !('queries' in input) &&
    Object.keys(input).length > 0
    ? { queries: [input] }
    : input;
}

/**
 * Advertise the canonical bulk schema unchanged, but let a batch whose
 * envelope is valid and that has at least one valid row reach the native
 * runtime, which executes the valid rows and returns indexed invalidInput
 * rows for the rest (CLI parity). Every other failure keeps the SDK issues.
 */
export function rowIsolatingSchema(
  inputSchema: StandardSchema,
  querySchema: { safeParse(value: unknown): { success: boolean } },
  envelopeSchema: { safeParse(value: unknown): { success: boolean } }
): StandardSchema {
  const standard = inputSchema['~standard'];
  if (!standard.jsonSchema) return inputSchema;
  return {
    '~standard': {
      version: standard.version,
      vendor: standard.vendor,
      jsonSchema: standard.jsonSchema,
      validate: async value => {
        const result = await standard.validate(value);
        if (!result.issues) return result;
        const envelope = wrapBareQuery(value);
        if (
          !envelope ||
          typeof envelope !== 'object' ||
          Array.isArray(envelope)
        )
          return result;
        const rows = (envelope as { queries?: unknown }).queries;
        if (!Array.isArray(rows) || rows.length < 2) return result;
        const valid = rows.filter(row => querySchema.safeParse(row).success);
        if (valid.length === 0 || valid.length === rows.length) return result;
        return envelopeSchema.safeParse({ ...envelope, queries: valid }).success
          ? { value: envelope }
          : result;
      },
    },
  };
}

type ToolDefinition = ReturnType<
  typeof getDirectToolDefinitionsWithAddons
>[number];

/**
 * The Standard Schema registered for a tool: bare queries are wrapped and
 * partially invalid batches reach native row isolation. The advertised
 * contract stays canonical. Clasify advertises bare-or-batch, but the MCP SDK
 * resolves its root object shape as the queries[] branch; preprocess its bare
 * form to that branch before SDK validation while preserving the union JSON
 * Schema shown to agents.
 */
export function toolInputSchema(
  definition: Pick<ToolDefinition, 'name' | 'schema' | 'inputSchema'>
): unknown {
  const bare = definition.name === 'clasify';
  const schema = bare
    ? (z.preprocess(
        wrapBareQuery,
        definition.inputSchema
      ) as unknown as StandardSchema)
    : rowIsolatingSchema(
        z.preprocess(
          wrapBareQuery,
          definition.inputSchema
        ) as unknown as StandardSchema,
        definition.schema,
        definition.inputSchema
      );
  return actionableIssuesSchema(schema, {
    normalize: wrapBareQuery,
    jsonSchema: () =>
      z.toJSONSchema(definition.inputSchema, {
        io: 'input',
        unrepresentable: 'any',
      }) as Record<string, unknown>,
  });
}

/**
 * Keep the schema's accept/reject decisions and advertised JSON Schema, but
 * rewrite rejection issues into the CLI's actionable wording (allowed enum
 * values, nearest field, missing field, valid field list).
 */
export function actionableIssuesSchema(
  inputSchema: StandardSchema,
  options: {
    normalize: (value: unknown) => unknown;
    jsonSchema: () => Record<string, unknown> | undefined;
  }
): StandardSchema {
  const standard = inputSchema['~standard'];
  if (!standard.jsonSchema) return inputSchema;
  return {
    '~standard': {
      version: standard.version,
      vendor: standard.vendor,
      jsonSchema: standard.jsonSchema,
      validate: async value => {
        const result = await standard.validate(value);
        if (!result.issues?.length) return result;
        try {
          const issues = formatIssues(result.issues as readonly RawIssue[], {
            value: options.normalize(value),
            jsonSchema: options.jsonSchema,
          });
          return issues.length ? { issues } : result;
        } catch {
          // Message shaping must never mask the underlying rejection.
          return result;
        }
      },
    },
  };
}

/**
 * MCP clients that read only `content` must still see the result. When the
 * native envelope carries structuredContent but no text block (clasify
 * receipts are not text-rendered), serialize structuredContent as the text
 * block, as the MCP spec recommends for structured results.
 */
export function ensureTextContent(result: unknown): unknown {
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
      `${nativeFingerprint}. Run \`yarn contracts:regen\` and rebuild native (or install ` +
      'matching octocode packages), or set OCTOCODE_ALLOW_CONTRACT_DRIFT=1 to override.';
    if (
      env.OCTOCODE_ALLOW_CONTRACT_DRIFT === '1' &&
      env.NODE_ENV !== 'production'
    ) {
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
  const server = new McpServer(implementation, {
    capabilities: { tools: { listChanged: false } },
    // Availability-scoped instructions, built by core from the tools the
    // native runtime actually enables — the native catalog carries none.
    instructions: buildMcpInstructions(
      availableTools.map(tool => tool.name),
      {
        grammarCapabilities: catalog.grammarCapabilities ?? [],
      }
    ),
  });
  const registerTool = server.registerTool.bind(server) as RegisterTool;

  const definitions = new Map(
    getDirectToolDefinitionsWithAddons({
      availableTools: availableTools.map(tool => tool.name),
    }).map(definition => [definition.name, definition])
  );

  for (const tool of availableTools) {
    const definition = definitions.get(tool.name);
    if (!definition) {
      void runtime.close();
      throw new Error(`Native catalog tool has no contract: ${tool.name}`);
    }
    const inputSchema = toolInputSchema(definition);
    registerTool(
      tool.name,
      {
        title: definition.title,
        description: definition.description,
        inputSchema,
      },
      async (args, context = {}) => {
        const signal = context.signal;
        const requestId = String(context.requestId ?? randomUUID());
        signal?.throwIfAborted();
        const cancel = () => runtime.cancel(requestId);
        signal?.addEventListener('abort', cancel, { once: true });
        try {
          return ensureTextContent(
            await runtime.executeMcp(requestId, tool.name, args)
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
            `[octocode-mcp] ${tool.name} execution error: ${detail}\n`
          );
          throw new Error(
            `Tool ${tool.name} failed to execute; see the server logs for detail.`,
            { cause: error }
          );
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
  // truncate a request mid-flight — but bound the drain so a stuck request cannot
  // hang the process past an orchestrator's grace window (which then SIGKILLs and
  // truncates anyway). Whichever of {drain complete, grace elapsed} comes first
  // exits cleanly.
  const SHUTDOWN_GRACE_MS = 10_000;
  const shutdown = (): void => {
    const forceExit = setTimeout(() => process.exit(0), SHUTDOWN_GRACE_MS);
    forceExit.unref?.();
    void instance.close().finally(() => {
      clearTimeout(forceExit);
      process.exit(0);
    });
  };
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
  process.stdin.once('end', shutdown);
  await instance.server.connect(new StdioServerTransport());
  return instance;
}
