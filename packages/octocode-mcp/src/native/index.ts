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
  publishedInputSchema,
  type GrammarCapability,
} from '@octocodeai/config/mcp';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import packageJson from '../../package.json';
import {
  coerceLosslessScalars,
  formatIssues,
  parseStringifiedQueries,
  type RawIssue,
} from './validationMessages.js';

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

/** Tools that are CLI-only by design and never registered over MCP. */
export const CLI_ONLY_TOOLS: ReadonlySet<string> = new Set([
  'ghCloneRepo',
  'astRewrite',
]);

// Replaced with `true` by the esbuild define in buildConfig.mjs; undefined when
// running from source (vitest, tsx).
declare const __OCTOCODE_BUNDLED__: boolean | undefined;

/**
 * Dev-only overrides (`OCTOCODE_NATIVE_BINDING`, `OCTOCODE_ALLOW_CONTRACT_DRIFT`)
 * are never honoured under `NODE_ENV=production`. The shipped bundle is
 * treated as production by default — `npx octocode-mcp` and registry installs
 * leave `NODE_ENV` unset — so it honours them only when `NODE_ENV` explicitly
 * opts in with `development` or `test`.
 */
export function devOverridesAllowed(
  env: NodeJS.ProcessEnv,
  bundled: boolean = typeof __OCTOCODE_BUNDLED__ !== 'undefined' &&
    __OCTOCODE_BUNDLED__ === true
): boolean {
  if (env.NODE_ENV === 'production') return false;
  if (!bundled) return true;
  return env.NODE_ENV === 'development' || env.NODE_ENV === 'test';
}

export function loadNativeBinding(
  env: NodeJS.ProcessEnv = process.env
): NativeRuntimeBinding {
  // `OCTOCODE_NATIVE_BINDING` require()s an arbitrary path (candidate-addon dev
  // aid). Honor it only outside production so a leaked/hostile env value cannot
  // load arbitrary code into a shipped server; production always resolves the
  // packaged addon.
  const override = devOverridesAllowed(env)
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
  },
  callback: (args: unknown, context?: ToolCallContext) => Promise<unknown>
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
 * Run `normalize` on the input before `inputSchema` validates it, so every
 * later step (row isolation, the value passed to native) sees one shape.
 */
export function normalizingSchema(
  inputSchema: StandardSchema,
  normalize: (value: unknown) => unknown
): StandardSchema {
  const standard = inputSchema['~standard'];
  return {
    '~standard': {
      ...standard,
      validate: value => standard.validate(normalize(value)),
    },
  };
}

/**
 * The Standard Schema registered for a tool: bare queries are wrapped, a
 * JSON-encoded `queries` array is parsed, lossless numeric/boolean strings
 * are coerced (CLI parity), and partially invalid batches reach native row
 * isolation. Validation uses the canonical contract; agents see core's slim
 * published view of it (a superset, sized for hosts that resend tools/list
 * every request). Clasify's bare matrix is wrapped into the queries[] branch
 * before SDK validation, which resolves its root object as that branch.
 */
export function toolInputSchema(
  definition: Pick<ToolDefinition, 'name' | 'schema' | 'inputSchema'>
): unknown {
  const canonical = z.toJSONSchema(definition.inputSchema, {
    io: 'input',
    unrepresentable: 'any',
  }) as Record<string, unknown>;
  const advertised = publishedInputSchema(definition.name, canonical);
  const normalize = (value: unknown) =>
    coerceLosslessScalars(
      parseStringifiedQueries(wrapBareQuery(value)),
      canonical
    );
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
  return actionableIssuesSchema(normalizingSchema(schema, normalize), {
    normalize,
    jsonSchema: () => canonical,
    advertised,
  });
}

/**
 * Keep the schema's accept/reject decisions, but rewrite rejection issues
 * into the CLI's actionable wording (allowed enum values, nearest field,
 * missing field, valid field list) from the canonical `jsonSchema`. When
 * `advertised` is set, tools/list shows it instead of the canonical input.
 */
export function actionableIssuesSchema(
  inputSchema: StandardSchema,
  options: {
    normalize: (value: unknown) => unknown;
    jsonSchema: () => Record<string, unknown> | undefined;
    advertised?: Record<string, unknown>;
  }
): StandardSchema {
  const standard = inputSchema['~standard'];
  if (!standard.jsonSchema) return inputSchema;
  const advertised = options.advertised;
  return {
    '~standard': {
      version: standard.version,
      vendor: standard.vendor,
      jsonSchema: advertised
        ? { ...(standard.jsonSchema as object), input: () => advertised }
        : standard.jsonSchema,
      validate: async value => {
        const result = await standard.validate(value);
        if (!result.issues?.length) return result;
        try {
          const issues = formatIssues(result.issues as readonly RawIssue[], {
            value: options.normalize(value),
            jsonSchema: options.jsonSchema,
            ...(advertised && { advertisedSchema: () => advertised }),
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
  // Second guard: CLI-only tools must never be exposed over MCP, even if the
  // native catalog reports them available.
  const availableTools = catalog.tools.filter(
    tool => tool.available && !CLI_ONLY_TOOLS.has(tool.name)
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
    const message =
      '[octocode-mcp] contract fingerprint mismatch: core ' +
      `${coreFingerprint} (@octocodeai/octocode-core) != native ` +
      `${nativeFingerprint}. Run \`yarn contracts:regen\` and rebuild native (or install ` +
      'matching octocode packages). To override while iterating locally, set ' +
      'OCTOCODE_ALLOW_CONTRACT_DRIFT=1; the bundled dist also needs ' +
      'NODE_ENV=development, and NODE_ENV=production always fails closed.';
    if (env.OCTOCODE_ALLOW_CONTRACT_DRIFT === '1' && devOverridesAllowed(env)) {
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
    // Hosts truncate instructions near 2 KB, so the grammar inventory stays
    // with `octocode scheme` rather than being appended here.
    instructions: buildMcpInstructions(availableTools.map(tool => tool.name)),
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
        const signal = context.mcpReq?.signal ?? context.signal;
        const requestId = String(
          context.mcpReq?.id ?? context.requestId ?? randomUUID()
        );
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
