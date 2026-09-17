import { mcpGatewayItemSchema } from './mcp/gateway-contract.js';
import { McpCatalogExecution } from './mcp/catalog-execution.js';
import { computeReload, createCatalogRefreshQueue } from './mcp/catalog-refresh.js';
export { computeReload } from './mcp/catalog-refresh.js';
import {
  formatMcpSchemaValidationErrors,
  renderMcpCall,
  renderMcpResult,
  summarizeMcpBatchResult,
  summarizeSchema,
} from './mcp/presentation.js';
export { formatMcpSchemaValidationErrors };
import { isWorkerCapabilityClient, dispatchWorkerMcpAction, getCurrentWorkerCapabilities } from './worker-capabilities.js';
import { readMcpCatalogPage } from './mcp/catalog-pages.js';
import { workerMcpCatalogSnapshot } from './mcp/worker-catalog.js';
import { DIRECT_TOOL_DESCRIPTIONS } from './octocode-tools.js';
import fs from "node:fs";
import path from "node:path";
import { registerMcpClientHandlers } from './mcp/client-handlers.js';
import { readOwnVersion } from '../package-metadata.js';
import {
  Client,
  StreamableHTTPClientTransport,
  type Transport,
} from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";
import {
  getMcpEnablement,
  listMcpOverrides,
  setMcpServerEnabled,
  setMcpToolEnabled,
} from "../contracts/mcp-state.js";
import { ensurePrivateDirectory } from '@octocodeai/octocode-awareness/host';
import { openOctocodeDb } from "./storage-policy.js";
import type {
  NotifyFn,
  PiContext,
  PiInstance,
  ToolCallResult,
  ToolDefinition,
} from "../types.js";
import { capMapSize } from "../utils.js";
import {
  DEFAULT_OCTOCODE_MCP_SERVER_NAME,
  buildServerHeaders,
  buildServerEnv,
  configSignature,
  globalMcpConfigPaths,
  globalMcpPath,
  isPlainRecord,
  loadMcpConfig,
  projectMcpConfigPaths,
  projectMcpPath,
  normalizeServerConfig,
  removeServerFromFile,
  requestOptions,
  resolveServerCwd,
  scopeTargetPath,
  upsertServerInFile,
  type McpLoadedConfig,
  type McpScope,
  type McpServerConfig,
} from "./mcp/config.js";

import { assertPathAllowed } from "./path-guard.js";
import {
  buildQueryEnvelopeSchema,
  executeQueryBatch,
  type QueryRecord,
} from "./query-envelope.js";
import { QueryBatchError } from './query-batch-error.js';
import { runSelectOverlay } from "./ui-overlays.js";
import {
  publishMcpRuntimeState,
  runtimeStoreFor,
  setManagedStatus,
} from "./runtime-renderer.js";
import { recordFileReadState } from "./file-state.js";
import {
  buildMcpCatalogSnapshot,
  measureMcpCatalog,
  readMcpCatalogSnapshot,
  renderMcpCatalogIndex,
  sameMcpCatalogContent,
  snapshotPathForWorkspace,
  stableSchemaDigest,
  writeMcpCatalogSnapshot,
  type McpCatalogServerInput,
  type McpCatalogSnapshotV1,
} from "./mcp/catalog.js";
import {
  McpSchemaUnsupportedError,
  compileMcpSchemaValidator,
  type McpCompiledSchemaValidator,
} from "./mcp/schema-validator.js";
import {
  createMcpOAuthFlow,
  revokeStoredMcpOAuthCredentials,
  type McpOAuthFlow,
} from "./mcp/oauth.js";
import { collectMcpPages, type McpCursorPage } from "./mcp/pagination.js";
import {
  resolveMcpCallContent,
  resolveMcpCallTable,
  summarizeMcpCallDetails,
  stringify,
} from "./mcp/sanitize.js";
import type {
  McpAction,
  McpConnection,
  ListedMcpServer,
  ValidatedMcpTool,
  McpDiscoveryServer,
  McpDiscoverySnapshot,
  McpPromptArtifactStatus,
} from "./mcp/types.js";

const MCP_STATUS_NAME = "octocode-mcp";
const MCP_DISCOVERY_ATTEMPT_TIMEOUT_MS = 7_500;
export const MCP_PROMPT_READY_TIMEOUT_MS = 35_000;
const connections = new Map<string, McpConnection>();
const pendingConnections = new Map<string, Promise<McpConnection>>();
const cachedCatalogs = new Map<string, ListedMcpServer[]>();
const cachedSnapshots = new Map<string, McpCatalogSnapshotV1>();
const cachedCatalogIndexes = new Map<string, string>();
const schemaCatalogs = new McpCatalogExecution<ListedMcpServer>();
const compiledValidators = new Map<string, McpCompiledSchemaValidator>();
const mcpSchemaMetrics = {
  snapshotHits: 0,
  snapshotMisses: 0,
  blockedCalls: 0,
};
/** In-flight init discoveries keyed by cwd, so turn 1 can await the warm started at session_start. */
const warmsInFlight = new Map<string, Promise<void>>();
const queuedCatalogRefreshes = createCatalogRefreshQueue({
  pending: key => warmsInFlight.get(key), refresh: ctx => warmMcpCatalog(ctx),
  onError: error => warnMcpWarmFailure(String(error)), track: work => trackMcpAsyncWork(work),
});

/** Coalesce invalidations behind the current discovery instead of reusing it. */
function queueMcpCatalogRefresh(ctx?: PiContext): void {
  queuedCatalogRefreshes.schedule(cacheKey(ctx), ctx);
}
/** Warm and close promises that session shutdown must drain before releasing its filesystem scope. */
const pendingMcpAsyncWork = new Set<Promise<unknown>>();

function trackMcpAsyncWork<T>(work: Promise<T>): Promise<T> {
  pendingMcpAsyncWork.add(work);
  void work.then(
    () => pendingMcpAsyncWork.delete(work),
    () => pendingMcpAsyncWork.delete(work),
  );
  return work;
}

export async function waitForMcpShutdown(): Promise<void> {
  while (pendingMcpAsyncWork.size > 0) {
    await Promise.allSettled([...pendingMcpAsyncWork]);
  }
}
/** Prompt readiness is intentionally separate from live refresh completion. A
 * matching persisted catalog snapshot resolves this barrier immediately while
 * live schema refresh continues in the background. */
const promptReadiness = new Map<string, Promise<boolean>>();
/**
 * Monotonic per-cwd generation for startup warms. A timeout or genuine cache
 * invalidation advances it so a stale async warm cannot repopulate prompt bytes.
 */
const warmGenerations = new Map<string, number>();
/** Bound the cwd-keyed caches so a long-lived process visiting many cwds cannot grow them without limit. */
const MAX_CACHED_CWDS = 32;
const MAX_DYNAMIC_MCP_PROXIES = 128;
const DYNAMIC_MCP_PROXY_PREFIX = "mcp__";

interface DynamicMcpProxyBinding {
  name: string;
  server: string;
  tool: string;
  schemaDigest: string;
}

interface DynamicMcpProxyState {
  supported: boolean;
  describedSchemas: Map<string, string>;
  bindingsByName: Map<string, DynamicMcpProxyBinding>;
  latestByIdentity: Map<string, DynamicMcpProxyBinding>;
  unavailableNames: Set<string>;
}

const dynamicMcpProxyStates = new WeakMap<PiInstance, DynamicMcpProxyState>();

function mcpToolIdentity(server: string, tool: string): string {
  return `${server}\u0000${tool}`;
}

function dynamicMcpProxySlug(value: string, maxLength: number): string {
  const slug = value.toLowerCase().replace(/[^a-z0-9_]+/g, "_").replace(/^_+|_+$/g, "");
  return (slug || "tool").slice(0, maxLength);
}

function dynamicMcpProxyName(server: string, tool: string, inputSchema: unknown): string {
  const digest = stableSchemaDigest({ server, tool, inputSchema }).slice(0, 12);
  return `${DYNAMIC_MCP_PROXY_PREFIX}${dynamicMcpProxySlug(server, 12)}__${dynamicMcpProxySlug(tool, 24)}__${digest}`;
}

function safeDynamicMcpText(value: string, maxLength: number): string {
  return value.replace(/[\u0000-\u001f\u007f]+/g, " ").replace(/\s+/g, " ").trim().slice(0, maxLength);
}

export function isDynamicMcpProxyTool(pi: PiInstance, name: string): boolean {
  return dynamicMcpProxyStates.get(pi)?.bindingsByName.has(name) ?? false;
}

export function getGrantedDynamicMcpProxyTools(
  pi: PiInstance,
  granted: ReadonlyArray<{ server: string; tool: string; inputSchema?: unknown }>,
): string[] {
  const state = dynamicMcpProxyStates.get(pi);
  if (!state) return [];
  const allowed = new Map(granted.map(item => [
    mcpToolIdentity(item.server, item.tool),
    item.inputSchema === undefined ? undefined : stableSchemaDigest(item.inputSchema),
  ]));
  return [...state.latestByIdentity.entries()]
    .filter(([identity, binding]) =>
      allowed.has(identity) &&
      state.describedSchemas.get(identity) === binding.schemaDigest &&
      (allowed.get(identity) === undefined || allowed.get(identity) === binding.schemaDigest))
    .map(([, binding]) => binding.name);
}


function cacheKey(ctx?: PiContext): string {
  return path.resolve(ctx?.cwd ?? process.cwd());
}

function warmGeneration(key: string): number {
  return warmGenerations.get(key) ?? 0;
}

function invalidateWarmResult(key: string): void {
  if (!warmsInFlight.has(key)) return;
  warmGenerations.set(key, warmGeneration(key) + 1);
}

function invalidateAllWarmResults(): void {
  for (const key of warmsInFlight.keys()) invalidateWarmResult(key);
}

async function ensureConnection(
  name: string,
  config: McpServerConfig,
  ctx?: PiContext,
  signal?: AbortSignal,
  timeoutMs?: number,
): Promise<McpConnection> {
  config = normalizeServerConfig(name, config);
  const sig = configSignature(config);
  const existing = connections.get(name);
  if (existing) {
    // Reuse only if the config is unchanged; otherwise the live process is stale —
    // reconnect with the new config so mcp.json edits apply without an agent restart.
    if (existing.configSig === sig) return existing;
    await stopConnection(name);
    invalidateServerCache(name);
  }
  // Dedupe concurrent connects for the same server: two parallel MCPTool calls
  // would otherwise both spawn a process and orphan one of them.
  const pending = pendingConnections.get(name);
  if (pending) {
    const conn = await pending;
    if (conn.configSig === sig) return conn;
  }
  const connectPromise = connectServer(
    name,
    config,
    sig,
    ctx,
    signal,
    false,
    timeoutMs,
  );
  pendingConnections.set(name, connectPromise);
  try {
    return await connectPromise;
  } finally {
    // A replacement session may already own a new connection attempt for the
    // same server. Only the promise that acquired this slot may release it.
    if (pendingConnections.get(name) === connectPromise) {
      pendingConnections.delete(name);
    }
  }
}

async function connectServer(
  name: string,
  config: McpServerConfig,
  sig: string,
  ctx?: PiContext,
  signal?: AbortSignal,
  oauthRetry = false,
  timeoutMs?: number,
): Promise<McpConnection> {
  let transport: Transport;
  let oauth: McpOAuthFlow | undefined;
  let stderr:
    | { on(event: string, listener: (chunk: Buffer) => void): unknown }
    | null
    | undefined;
  if (config.transport === "http" || config.url) {
    if (config.auth === "oauth")
      oauth = await createMcpOAuthFlow(name, config.url!, ctx);
    transport = new StreamableHTTPClientTransport(new URL(config.url!), {
      requestInit: { headers: buildServerHeaders(config) },
      ...(oauth ? { authProvider: oauth.provider } : {}),
    });
    if (oauth)
      oauth.attachTransport(transport as StreamableHTTPClientTransport);
  } else {
    const cwd = resolveServerCwd(config, ctx);
    assertPathAllowed(cwd, ctx?.cwd ?? process.cwd(), `mcp:${name}`);
    const stdio = new StdioClientTransport({
      command: config.command!,
      args: config.args ?? [],
      cwd,
      env: buildServerEnv(name, config),
      stderr: "pipe",
    });
    transport = stdio;
    stderr = stdio.stderr;
  }
  const client = new Client(
    { name: "octocode-pi-extension", version: readOwnVersion() ?? "unknown" },
    {
      capabilities: {
        roots: { listChanged: true },
        sampling: {},
        elicitation: { form: {}, url: {} },
      },
      inputRequired: { autoFulfill: true, maxRounds: 8 },
      versionNegotiation: { mode: "auto" },
      listChanged: {
        tools: { onChanged: () => refreshChangedMcpServer(name, ctx) },
        prompts: { onChanged: () => refreshChangedMcpServer(name, ctx) },
        resources: { onChanged: () => refreshChangedMcpServer(name, ctx) },
      },
    },
  );
  registerMcpClientHandlers(client, name, ctx, () => {
    invalidateServerCache(name);
    markMcpPromptStale(ctx);
    queueMcpCatalogRefresh(ctx);
  });
  const connection: McpConnection = {
    name,
    config,
    configSig: sig,
    client,
    transport,
    stderr: [],
    startedAt: Date.now(),
    ...(oauth ? { oauth } : {}),
  };
  stderr?.on("data", (chunk: Buffer) => {
    const text = chunk.toString("utf8").trim();
    if (!text) return;
    connection.stderr.push(text);
    while (connection.stderr.length > 20) connection.stderr.shift();
  });
  transport.onclose = () => {
    // Delete only our own entry — a reconnect may already own the slot.
    if (connections.get(name) === connection) connections.delete(name);
    connection.oauth?.close();
  };
  transport.onerror = (error) => {
    connection.stderr.push(error.message);
  };
  try {
    await client.connect(
      transport,
      requestOptions(
        { ...config, timeoutMs: config.startupTimeoutMs ?? timeoutMs ?? config.timeoutMs },
        signal,
      ),
    );
  } catch (error) {
    const stderrText =
      connection.stderr.length > 0
        ? `\nstderr:\n${connection.stderr.join("\n")}`
        : "";
    await client.close().catch(() => undefined);
    const authorized =
      oauth && !oauthRetry ? await oauth.hasTokens().catch(() => false) : false;
    oauth?.close();
    if (authorized)
      return connectServer(name, config, sig, ctx, signal, true, timeoutMs);
    throw new Error(`${(error as Error).message}${stderrText}`);
  }
  connections.set(name, connection);
  return connection;
}

function refreshChangedMcpServer(name: string, ctx?: PiContext): void {
  invalidateServerCache(name);
  invalidateCwdCache(ctx);
  runtimeStoreFor(ctx)
    ?.getState()
    .announce(
      `MCP ${name}: catalog changed; refreshing descriptions and schemas.`,
      "info",
    );
  queueMcpCatalogRefresh(ctx);
}

async function stopConnection(name: string): Promise<boolean> {
  const connection = connections.get(name);
  if (!connection) return false;
  connections.delete(name);
  connection.oauth?.close();
  await connection.client.close().catch(() => undefined);
  return true;
}

export function isMcpServerConnected(name: string): boolean {
  return connections.has(name);
}

export function stopAllMcpServers(): number {
  const names = [...connections.keys()];
  for (const name of names) {
    const connection = connections.get(name);
    connections.delete(name);
    connection?.oauth?.close();
    if (connection) trackMcpAsyncWork(connection.client.close().catch(() => undefined));
  }
  // Drop the injected-catalog cache so a following session in the same process
  // (/new, /resume) cannot serve tools from now-stopped servers in the system
  // prompt. Invalidate pending warm generations before clearing so an old async
  // result cannot repopulate the new session's prompt after shutdown.
  invalidateAllWarmResults();
  // The entries are workspace keyed, but their promises are session-context
  // bound. Detach them now so /new, /resume, and /fork can install work owned by
  // the replacement context instead of awaiting a stale Pi context.
  warmsInFlight.clear();
  queuedCatalogRefreshes.clear();
  promptReadiness.clear();
  pendingConnections.clear();
  cachedCatalogs.clear();
  cachedSnapshots.clear();
  cachedCatalogIndexes.clear();
  schemaCatalogs.clear();
  compiledValidators.clear();
  return names.length;
}

// ─── mcp.json file watcher: hot-reload on external edits ──────────────────────
// The config is already re-read per MCPTool call and connections auto-reconnect on
// drift; the watcher makes that PROACTIVE — it detects external mcp.json edits, drops
// stale connections + cache immediately, and tells the user, so a long-idle connection
// never lingers on old config and the model-facing catalog addendum stays honest.

const configWatchers: import("node:fs").FSWatcher[] = [];
let watchDebounce: ReturnType<typeof setTimeout> | null = null;

async function reconcileMcpConfig(
  ctx: PiContext | undefined,
  notify: NotifyFn,
): Promise<void> {
  try {
    const loaded = await loadMcpConfig(ctx);
    const running = new Map<string, string>();
    for (const [name, conn] of connections) running.set(name, conn.configSig);
    const { changed, removed } = computeReload(running, loaded.servers);
    for (const name of [...changed, ...removed]) {
      await stopConnection(name);
      invalidateServerCache(name);
    }
    invalidateCwdCache(ctx);
    markMcpPromptStale(ctx);
    queueMcpCatalogRefresh(ctx);
    if (changed.length || removed.length) {
      const parts: string[] = [];
      if (changed.length) parts.push(`reloaded ${changed.join(", ")}`);
      if (removed.length) parts.push(`removed ${removed.join(", ")}`);
      notify(
        ctx,
        `MCP config changed — ${parts.join("; ")}. Execution and routing refresh automatically; the next turn receives the current catalog.`,
        "info",
      );
    }
  } catch {
    // Best-effort: a bad transient config write must not crash the watcher.
  }
}

/**
 * Start watching all active global + project mcp.json directories for changes.
 * Debounced and best-effort (watching is disabled silently if the platform/dir
 * does not allow it). Call stopMcpConfigWatchers() on session shutdown.
 */
export function startMcpConfigWatcher(
  ctx: PiContext | undefined,
  notify: NotifyFn,
): number {
  if (isWorkerCapabilityClient()) return 0;
  stopMcpConfigWatchers();
  const cwd = ctx?.cwd ?? process.cwd();
  const dirs = new Set([
    ...globalMcpConfigPaths().map((filePath) => path.dirname(filePath)),
    ...projectMcpConfigPaths(cwd).map((filePath) => path.dirname(filePath)),
  ]);
  const canonicalGlobalDir = path.dirname(globalMcpPath());
  for (const dir of dirs) {
    try {
      // Keep the existing management target available. Alias and project dirs
      // are watched only when present; watcher setup must not create them.
      if (dir === canonicalGlobalDir) ensurePrivateDirectory(dir);
      else if (!fs.existsSync(dir)) continue;
      const watcher = fs.watch(
        dir,
        { persistent: false },
        (_event: string, filename: string | Buffer | null) => {
          // Match mcp.json and our atomic temp writes (mcp.json.<pid>.<ts>.tmp).
          if (filename && !String(filename).startsWith("mcp.json")) return;
          if (watchDebounce) clearTimeout(watchDebounce);
          watchDebounce = setTimeout(() => {
            void reconcileMcpConfig(ctx, notify);
          }, 250);
        },
      );
      configWatchers.push(watcher);
    } catch {
      // Watching is best-effort; per-call re-read + drift reconnect remain the safety net.
    }
  }
  return configWatchers.length;
}

export function stopMcpConfigWatchers(): number {
  const count = configWatchers.length;
  for (const watcher of configWatchers) {
    try {
      watcher.close();
    } catch {
      /* already closed */
    }
  }
  configWatchers.length = 0;
  if (watchDebounce) {
    clearTimeout(watchDebounce);
    watchDebounce = null;
  }
  return count;
}


function result(
  text: string,
  details?: unknown,
  isError = false,
): ToolCallResult {
  return { content: [{ type: "text", text }], details, isError };
}

function schemaRequiredMessage(server: string, tool: string): string {
  return [
    `MCP_SCHEMA_REQUIRED ${server}/${tool}`,
    "Load the exact schema before calling through MCPTool:",
    JSON.stringify({
      tool: "MCPTool",
      params: {
        queries: [{
          action: "describe",
          server,
          tool,
        }],
      },
    }),
  ].join("\n");
}

function activateDescribedMcpProxy(
  pi: PiInstance,
  state: DynamicMcpProxyState,
  described: ToolCallResult,
  ctx?: PiContext,
): ToolCallResult {
  if (described.isError || !isPlainRecord(described.details)) return described;
  const server = described.details["server"];
  const rawTool = described.details["tool"];
  if (typeof server !== "string" || !isPlainRecord(rawTool)) return described;
  const tool = rawTool["name"];
  const inputSchema = rawTool["inputSchema"];
  if (typeof tool !== "string" || !isPlainRecord(inputSchema)) return described;

  const identity = mcpToolIdentity(server, tool);
  const schemaDigest = stableSchemaDigest(inputSchema);
  state.describedSchemas.set(identity, schemaDigest);
  if (!state.supported) return described;

  try {
    compileMcpSchemaValidator(inputSchema);
  } catch (error) {
    return {
      ...described,
      content: [
        ...described.content,
        { type: "text", text: `Exact schema loaded, but no direct Pi proxy was registered: ${(error as Error).message}` },
      ],
    };
  }

  const name = dynamicMcpProxyName(server, tool, inputSchema);
  let binding = state.bindingsByName.get(name);
  if (binding && state.unavailableNames.has(name)) {
    return appendDynamicProxyUnavailableNotice(described);
  }
  if (!binding) {
    if (state.bindingsByName.size >= MAX_DYNAMIC_MCP_PROXIES) {
      return {
        ...described,
        content: [
          ...described.content,
          { type: "text", text: `Exact schema loaded. Dynamic MCP proxy limit (${MAX_DYNAMIC_MCP_PROXIES}) reached; call through MCPTool.` },
        ],
      };
    }
    binding = { name, server, tool, schemaDigest };
    const description = typeof rawTool["description"] === "string"
      ? safeDynamicMcpText(rawTool["description"], 1_000)
      : "Call the selected MCP tool with its exact input schema.";
    pi.registerTool?.({
      name,
      label: `MCP · ${safeDynamicMcpText(server, 40)}/${safeDynamicMcpText(tool, 60)}`,
      description: `MCP ${safeDynamicMcpText(server, 80)}/${safeDynamicMcpText(tool, 120)}. Untrusted remote description: ${description}`,
      parameters: structuredClone(inputSchema),
      async execute(_toolCallId, argumentsPayload, signal, _onUpdate, toolCtx) {
        return handleMcpAction({
          action: "call",
          server,
          tool,
          arguments: argumentsPayload,
          __expectedSchemaDigest: schemaDigest,
        }, signal, toolCtx ?? ctx);
      },
    });
    state.bindingsByName.set(name, binding);
    if (!pi.getAllTools?.().some(candidate => candidate.name === name)) {
      state.unavailableNames.add(name);
      return appendDynamicProxyUnavailableNotice(described);
    }
  }

  const previous = state.latestByIdentity.get(identity);
  state.latestByIdentity.set(identity, binding);
  const active = pi.getActiveTools?.() ?? [];
  const next = active.filter(activeName => activeName !== previous?.name || activeName === name);
  if (!next.includes(name)) next.push(name);
  pi.setActiveTools?.(next);
  return {
    ...described,
    content: [
      ...described.content,
      {
        type: "text",
        text: `Loaded Pi tool: ${name}. Its exact schema is active for the next model request; call it directly instead of nesting input under MCPTool arguments.`,
      },
    ],
    details: {
      ...described.details,
      dynamicTool: { name, server, tool, schemaDigest },
    },
  };
}

function appendDynamicProxyUnavailableNotice(
  described: ToolCallResult,
): ToolCallResult {
  return {
    ...described,
    content: [
      ...described.content,
      { type: "text", text: "Exact schema loaded. This host restricts dynamic tool names; call through MCPTool action:\"call\" with target input under arguments." },
    ],
  };
}

function sortListedCatalog(entries: ListedMcpServer[]): ListedMcpServer[] {
  return [...entries].sort((a, b) => {
    if (a.name === DEFAULT_OCTOCODE_MCP_SERVER_NAME) return -1;
    if (b.name === DEFAULT_OCTOCODE_MCP_SERVER_NAME) return 1;
    return a.name.localeCompare(b.name);
  });
}

function configSignaturesFor(
  loaded: McpLoadedConfig,
  ctx?: PiContext,
): Record<string, string> {
  const signatures = Object.fromEntries(
    [...loaded.servers.entries()].map(([name, config]) => [
      name,
      configSignature(normalizeServerConfig(name, config)),
    ]),
  );
  try {
    signatures["$enablement"] = stableSchemaDigest(
      listMcpOverrides(
        openOctocodeDb(),
        path.resolve(ctx?.cwd ?? process.cwd()),
      ),
    );
  } catch {
    signatures["$enablement"] = "unavailable";
  }
  return signatures;
}

function snapshotFromListed(
  ctx: PiContext | undefined,
  entries: ListedMcpServer[],
  options: { loaded?: McpLoadedConfig; capturedAt?: string } = {},
): McpCatalogSnapshotV1 {
  const configSignatures = options.loaded
    ? configSignaturesFor(options.loaded, ctx)
    : Object.fromEntries(
        entries.map((entry) => [
          entry.name,
          entry.configSignature ?? `test:${entry.name}`,
        ]),
      );
  const scopeKey = path.resolve(ctx?.cwd ?? process.cwd());
  let db: ReturnType<typeof openOctocodeDb> | undefined;
  try {
    db = openOctocodeDb();
  } catch {
    /* catalog stays available if the DB is unavailable */
  }
  const servers: McpCatalogServerInput[] = entries.filter(entry => !options.loaded || options.loaded.servers.has(entry.name)).map((entry) => {
    return {
      name: entry.name,
      ...(entry.instructions ? { instructions: entry.instructions } : {}),
      tools: entry.tools
        .filter(isPlainRecord)
        .filter(
          (tool) =>
            typeof tool["name"] === "string" &&
            Object.hasOwn(tool, "inputSchema"),
        )
        .filter(tool => isConfiguredMcpToolEnabled(options.loaded?.configuredServers.get(entry.name), String(tool['name'])))
        .filter((tool) =>
          db
            ? getMcpEnablement(
                db,
                scopeKey,
                entry.name,
                String(tool["name"]),
                true,
              )
            : true,
        )
        .map((tool) => ({
          name: String(tool["name"]),
          ...(typeof tool["description"] === "string"
            ? { description: tool["description"] }
            : {}),
          inputSchema: tool["inputSchema"],
        })),
    };
  });
  return buildMcpCatalogSnapshot({
    cwd: ctx?.cwd ?? process.cwd(),
    sources: (options.loaded?.sources ?? []).map((source) => ({
      scope: source.scope,
      path: source.path,
    })),
    configSignatures,
    servers,
    ...(options.capturedAt ? { capturedAt: options.capturedAt } : {}),
  });
}

function cachePromptSnapshot(
  ctx: PiContext | undefined,
  snapshot: McpCatalogSnapshotV1,
  routingIndex?: string,
): void {
  const key = cacheKey(ctx);
  cachedSnapshots.delete(key);
  cachedSnapshots.set(key, snapshot);
  capMapSize(cachedSnapshots, MAX_CACHED_CWDS);
  cachedCatalogIndexes.delete(key);
  cachedCatalogIndexes.set(
    key,
    routingIndex ?? renderMcpCatalogIndex(snapshot),
  );
  capMapSize(cachedCatalogIndexes, MAX_CACHED_CWDS);
}

function cacheListedCatalog(
  ctx: PiContext | undefined,
  listed: ListedMcpServer[],
  options: {
    loaded?: McpLoadedConfig;
    updatePromptSnapshot?: boolean;
    promptIndex?: string;
  } = {},
): ListedMcpServer[] {
  const key = cacheKey(ctx);
  const now = Date.now();
  const existing = new Map(
    (cachedCatalogs.get(key) ?? []).map((entry) => [entry.name, entry]),
  );
  if (options.loaded) {
    for (const [name, entry] of existing) {
      const config = options.loaded.servers.get(name);
      if (!config || entry.configSignature !== configSignature(normalizeServerConfig(name, config))) existing.delete(name);
    }
  }
  for (const entry of listed) {
    const config = options.loaded?.servers.get(entry.name);
    existing.set(entry.name, {
      ...entry,
      ...(config
        ? {
            configSignature: configSignature(
              normalizeServerConfig(entry.name, config),
            ),
          }
        : {}),
      cachedAt: now,
    });
  }
  const merged = sortListedCatalog([...existing.values()]);
  // delete-then-set makes the cwd the most-recently-used key, so capMapSize evicts the coldest cwd.
  cachedCatalogs.delete(key);
  cachedCatalogs.set(key, merged);
  capMapSize(cachedCatalogs, MAX_CACHED_CWDS);
  if (options.updatePromptSnapshot !== false)
    cachePromptSnapshot(
      ctx,
      snapshotFromListed(ctx, merged, { loaded: options.loaded }),
      options.promptIndex,
    );
  return merged;
}

function listedFromSnapshot(snapshot: McpCatalogSnapshotV1): ListedMcpServer[] {
  return snapshot.servers.map((server) => ({
    name: server.name,
    configSignature: server.configSignature,
    ...(server.instructions ? { instructions: server.instructions } : {}),
    tools: server.tools.map((tool) => ({
      name: tool.name,
      ...(tool.description ? { description: tool.description } : {}),
      inputSchema: tool.inputSchema,
    })),
    text: `${server.name}: ${server.tools.length} tool(s)`,
  }));
}

/** Drop the entire cached catalog for a cwd (used when servers are added/removed/stopped). */
function invalidateCwdCache(ctx?: PiContext): void {
  const key = cacheKey(ctx);
  invalidateWarmResult(key);
  cachedCatalogs.delete(key);
  cachedSnapshots.delete(key);
  cachedCatalogIndexes.delete(key);
  schemaCatalogs.invalidateWorkspace(key);
  compiledValidators.clear();
}

/**
 * Drop one server from every cached catalog. Used on restart / stop / config-drift /
 * tools/list_changed, where we lack the originating cwd but must not serve a stale entry.
 */
function invalidateServerCache(name: string): void {
  // Server connections are process-global, so a server-level invalidation may
  // affect every cwd currently warming or rendering that server.
  invalidateAllWarmResults();
  for (const [key, entries] of cachedCatalogs) {
    const next = entries.filter((entry) => entry.name !== name);
    if (next.length !== entries.length) {
      cachedSnapshots.delete(key);
      cachedCatalogIndexes.delete(key);
      if (next.length === 0) cachedCatalogs.delete(key);
      else cachedCatalogs.set(key, next);
    }
  }
  schemaCatalogs.invalidateServer(name);
  compiledValidators.clear();
}

function capCatalogText(text: string, cap: number): string {
  return text.length <= cap ? text : `${text.slice(0, cap)}…`;
}

function warnMcpWarmFailure(message: string): void {
  try {
    process.stderr.write(`[octocode-mcp] ${message}\n`);
  } catch {
    /* stderr unavailable */
  }
}

function notifyMcpWarm(
  ctx: PiContext | undefined,
  message: string,
  level: "info" | "warning" = "info",
): void {
  const store = runtimeStoreFor(ctx);
  if (store) {
    if (store.getState().phase !== "initializing")
      store.getState().announce(message, level);
    return;
  }
  try {
    ctx?.ui?.notify?.(message, level);
  } catch {
    /* UI unavailable */
  }
}


async function persistMcpArtifacts(
  snapshot: McpCatalogSnapshotV1,
  options: { home?: string } = {},
): Promise<{ snapshotPath: string }> {
  return {
    snapshotPath: await writeMcpCatalogSnapshot(snapshot, {
      ...(options.home ? { home: options.home } : {}),
    }),
  };
}
/**
 * Warm MCP discovery once per workspace. A matching exact snapshot is prompt-ready
 * immediately through its deterministic routing index; live discovery publishes
 * updated execution contracts and the next turn's index.
 */
export function warmMcpCatalog(
  ctx?: PiContext,
  signal?: AbortSignal,
): Promise<void> {
  if (isWorkerCapabilityClient()) return Promise.resolve();
  const key = cacheKey(ctx);
  const existing = warmsInFlight.get(key);
  if (existing) return existing;
  const generation = warmGeneration(key);
  const isCurrentWarm = (): boolean =>
    !signal?.aborted && warmGeneration(key) === generation;
  let resolvePromptReady: (ready: boolean) => void = () => undefined;
  const promptReady = new Promise<boolean>((resolve) => {
    resolvePromptReady = resolve;
  });
  promptReadiness.set(key, promptReady);
  let promptReadySettled = false;
  const settlePromptReady = (ready: boolean): void => {
    if (promptReadySettled) return;
    promptReadySettled = true;
    resolvePromptReady(ready);
  };
  const warm = (async (): Promise<void> => {
    const listed: ListedMcpServer[] = [];
    try {
      const loaded = await loadMcpConfig(ctx);
      publishMcpRuntimeState(ctx, {
        status: "running",
        message: "checking cache",
        servers: loaded.servers.size,
        tools: 0,
        totalServers: loaded.servers.size,
        completedServers: 0,
        failedServers: [],
        currentServer: undefined,
      });
      const identity = snapshotFromListed(ctx, [], { loaded });
      const validateCurrentConfig = async (): Promise<boolean> => {
        const currentLoaded = await loadMcpConfig(ctx);
        if (!isCurrentWarm()) return false;
        if (snapshotFromListed(ctx, [], { loaded: currentLoaded }).configDigest === identity.configDigest) return true;
        cacheListedCatalog(ctx, [], { loaded: currentLoaded });
        if (currentLoaded.servers.size > 0) queueMcpCatalogRefresh(ctx);
        return false;
      };
      const persisted = await readMcpCatalogSnapshot({
        workspaceKey: identity.workspaceKey,
        configDigest: identity.configDigest,
      });
      const persistedPrompt = persisted
        ? renderMcpCatalogIndex(persisted)
        : undefined;
      if (persisted && persistedPrompt) {
        mcpSchemaMetrics.snapshotHits += 1;
        cachePromptSnapshot(ctx, persisted, persistedPrompt);
        cachedCatalogs.set(key, listedFromSnapshot(persisted));
        capMapSize(cachedCatalogs, MAX_CACHED_CWDS);
        const toolCount = persisted.servers.reduce(
          (sum, server) => sum + server.tools.length,
          0,
        );
        publishMcpRuntimeState(ctx, {
          status: "ready",
          source: "cache",
          servers: persisted.servers.length,
          tools: toolCount,
          totalServers: loaded.servers.size,
          completedServers: loaded.servers.size,
          failedServers: [],
          currentServer: undefined,
          message: "cached routing index ready",
        });
        settlePromptReady(true);
        notifyMcpWarm(
          ctx,
          `MCP ready: using cached catalog (${persisted.servers.length} server(s), ${persisted.servers.reduce((sum, server) => sum + server.tools.length, 0)} tool(s)).`,
        );
      } else {
        mcpSchemaMetrics.snapshotMisses += 1;
        publishMcpRuntimeState(ctx, {
          status: "running",
          source: "none",
          servers: loaded.servers.size,
          tools: 0,
          totalServers: loaded.servers.size,
          completedServers: 0,
          failedServers: [],
          currentServer: undefined,
          message: "discovering tools",
        });
        notifyMcpWarm(
          ctx,
          "MCP configuration changed or cache is missing; discovering enabled tools and exact input schemas…",
        );
      }
      const serverEntries = [...loaded.servers];
      let completedServers = 0;
      const failedServers: string[] = [];
      const activeServers = new Set<string>();
      const discoveries = await Promise.allSettled(
        serverEntries.map(async ([name, config]) => {
          const discoveryTimeoutMs = Math.min(
            config.timeoutMs ?? MCP_DISCOVERY_ATTEMPT_TIMEOUT_MS,
            MCP_DISCOVERY_ATTEMPT_TIMEOUT_MS,
          );
          activeServers.add(name);
          publishMcpRuntimeState(ctx, {
            currentServer: name,
            message: "discovering tools",
          });
          try {
            try {
              return await listServerTools(
                name,
                config,
                ctx,
                signal,
                discoveryTimeoutMs,
              );
            } catch (firstError) {
              if (signal?.aborted) throw firstError;
              publishMcpRuntimeState(ctx, {
                currentServer: name,
                message: "retrying discovery",
              });
              await stopConnection(name);
              return await listServerTools(
                name,
                config,
                ctx,
                signal,
                discoveryTimeoutMs,
              );
            }
          } catch (error) {
            failedServers.push(name);
            throw error;
          } finally {
            activeServers.delete(name);
            completedServers += 1;
            publishMcpRuntimeState(ctx, {
              completedServers,
              failedServers: [...failedServers].sort(),
              currentServer: [...activeServers].sort()[0],
            });
          }
        }),
      );
      if (!isCurrentWarm()) return;
      for (const discovery of discoveries) {
        // Best-effort per server: a slow/broken MCP must not prevent the rest of
        // the catalog from being cached or block session start.
        if (discovery.status === "fulfilled") listed.push(discovery.value);
      }
      // The filesystem or enablement may change while a server is answering.
      // Never publish the old config's results into the current prompt/grants.
      if (!await validateCurrentConfig()) return;
      if (listed.length > 0) {
        const refreshed = snapshotFromListed(ctx, listed, { loaded });
        let promptIndex = persistedPrompt;
        if (
          !persisted ||
          !persistedPrompt ||
          !sameMcpCatalogContent(persisted, refreshed)
        ) {
          promptIndex = renderMcpCatalogIndex(refreshed);
          await persistMcpArtifacts(refreshed)
            .then(({ snapshotPath }) => {
              notifyMcpWarm(
                ctx,
                `MCP ready: saved exact enabled catalog to ${snapshotPath}.`,
              );
            })
            .catch((error) => {
              warnMcpWarmFailure(
                `catalog snapshot write failed: ${(error as Error).message}`,
              );
            });
        }
        // Publish only into the session generation that owns this discovery.
        if (await validateCurrentConfig()) {
          cacheListedCatalog(ctx, listed, {
            loaded,
            updatePromptSnapshot: true,
            promptIndex,
          });
          const toolCount = listed.reduce(
            (sum, server) => sum + server.tools.length,
            0,
          );
          publishMcpRuntimeState(ctx, {
            status: failedServers.length > 0 ? "degraded" : "ready",
            source: persistedPrompt ? "cache" : "generated",
            servers: listed.length,
            tools: toolCount,
            totalServers: loaded.servers.size,
            completedServers: loaded.servers.size,
            failedServers: [...failedServers].sort(),
            currentServer: undefined,
            message:
              failedServers.length > 0
                ? `catalog ready with ${failedServers.length} failed server${failedServers.length === 1 ? "" : "s"}`
                : "catalog ready",
          });
          settlePromptReady(true);
        }
      } else if (
        loaded.servers.size > 0 &&
        warmGeneration(key) === generation
      ) {
        publishMcpRuntimeState(ctx, {
          status: "degraded",
          source: "none",
          servers: 0,
          tools: 0,
          totalServers: loaded.servers.size,
          completedServers: loaded.servers.size,
          failedServers: [...failedServers].sort(),
          currentServer: undefined,
          message: "no enabled MCP server could be discovered",
        });
        settlePromptReady(false);
      }
      // A late cache miss is deliberately persisted above but never injected into
      // this session after the first-turn deadline invalidates the generation.
    } catch (err) {
      const staleContext =
        err instanceof Error &&
        err.message.includes("extension ctx is stale after session replacement or reload");
      const superseded = !isCurrentWarm() || staleContext;
      if (!superseded) {
        // Best-effort: a missing/unreadable MCP config must not block session start,
        // but a genuine load error (e.g. malformed mcp.json) is worth surfacing.
        warnMcpWarmFailure(
          `catalog warm failed: ${(err as Error)?.message ?? String(err)}`,
        );
        publishMcpRuntimeState(ctx, {
          status: "degraded",
          source: "none",
          message: "ready with warnings",
        });
      }
      settlePromptReady(false);
    }
  })().finally(() => {
    settlePromptReady(Boolean(cachedCatalogs.get(key)?.length));
    if (warmsInFlight.get(key) === warm) {
      warmsInFlight.delete(key);
      warmGenerations.delete(key);
      promptReadiness.delete(key);
    } else if (
      !warmsInFlight.has(key) &&
      warmGeneration(key) !== generation
    ) {
      // Shutdown detached this superseded promise and no replacement adopted
      // the workspace slot. Avoid retaining a generation tombstone forever.
      warmGenerations.delete(key);
    }
  });
  trackMcpAsyncWork(warm);
  warmsInFlight.set(key, warm);
  return warm;
}

/** Bounded startup wait. Late discovery updates the next turn's projection. */
export async function mcpCatalogReady(
  ctx?: PiContext,
  timeoutMs = MCP_PROMPT_READY_TIMEOUT_MS,
): Promise<boolean> {
  const key = cacheKey(ctx);
  if (cachedCatalogs.get(key)?.length) return true;
  const pending = promptReadiness.get(key);
  if (!pending) return false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let completed = false;
  try {
    completed = await Promise.race([
      pending,
      new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(false), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
  const ready = completed && Boolean(cachedCatalogs.get(key)?.length);
  // A slow initialization may publish a new revision for the next turn. Keep
  // the in-flight connection; timeout does not cancel or duplicate discovery.
  return ready;
}

/** Cheap counts (servers + total tools) from the in-memory MCP catalog cache. */
export function getCachedMcpCounts(ctx?: PiContext): {
  servers: number;
  tools: number;
} {
  const cached = getEffectiveMcpSnapshot(ctx)?.servers;
  if (!cached?.length) return { servers: 0, tools: 0 };
  const tools = cached.reduce(
    (sum, entry) => sum + (Array.isArray(entry.tools) ? entry.tools.length : 0),
    0,
  );
  return { servers: cached.length, tools };
}

export function getCachedMcpCatalogAddendum(ctx?: PiContext): string {
  if (isWorkerCapabilityClient()) {
    const snapshot = getEffectiveMcpSnapshot(ctx);
    return snapshot ? renderMcpCatalogIndex(snapshot) : '';
  }
  const key = cacheKey(ctx);
  return cachedCatalogIndexes.get(key) ?? "";
}

/** Reconcile removals immediately; new/changed servers publish after discovery. */
export async function refreshMcpCapabilities(ctx?: PiContext): Promise<void> {
  if (isWorkerCapabilityClient()) return;
  const loaded = await loadMcpConfig(ctx);
  const running = new Map([...connections].map(([name, connection]) => [name, connection.configSig]));
  const { changed, removed } = computeReload(running, loaded.servers);
  for (const name of [...changed, ...removed]) {
    await stopConnection(name);
    invalidateServerCache(name);
  }
  const entries = cacheListedCatalog(ctx, [], { loaded });
  if ([...loaded.servers.keys()].some(name => !entries.some(entry => entry.name === name))) {
    if (warmsInFlight.has(cacheKey(ctx))) {
      if (changed.length || removed.length) queueMcpCatalogRefresh(ctx);
    }
    else void warmMcpCatalog(ctx);
  }
  await mcpCatalogReady(ctx);
}

export function getEffectiveMcpSnapshot(ctx?: PiContext): McpCatalogSnapshotV1 | undefined {
  if (isWorkerCapabilityClient()) {
    const view = getCurrentWorkerCapabilities();
    if (!view) return undefined;
    return workerMcpCatalogSnapshot(view.snapshot, ctx?.cwd ?? process.cwd());
  }
  const snapshot = cachedSnapshots.get(cacheKey(ctx));
  return snapshot ? structuredClone(snapshot) : undefined;
}

/** Configuration inspection includes disabled tools; model projection never does. */
export function getMcpServerInspection(server: string, ctx?: PiContext): ListedMcpServer | undefined {
  const entry = cachedCatalogs.get(cacheKey(ctx))?.find(item => item.name === server);
  return entry ? structuredClone(entry) : undefined;
}

export function isConfiguredMcpToolEnabled(config: McpServerConfig | undefined, tool: string): boolean {
  return !config?.disabledTools?.includes(tool) && (!config?.enabledTools || config.enabledTools.includes(tool));
}


export function getMcpPromptArtifactStatus(ctx?: PiContext): McpPromptArtifactStatus {
  const key = cacheKey(ctx);
  const snapshot = cachedSnapshots.get(key);
  const promptChars = cachedCatalogIndexes.get(key)?.length ?? 0;
  if (!snapshot) return { mode: "routing", status: "pending", promptChars };
  return {
    mode: "routing",
    status: "ready",
    promptChars,
    workspaceKey: snapshot.workspaceKey,
    configDigest: snapshot.configDigest,
    capturedAt: snapshot.capturedAt,
    catalogPath: snapshotPathForWorkspace(snapshot.workspaceKey),
  };
}

function markMcpPromptStale(ctx?: PiContext): void {
  const store = runtimeStoreFor(ctx);
  if (!store || store.getState().context.status !== "ready") return;
  store.getState().setContext({ status: "stale" });
  store
    .getState()
    .announce(
      "MCP capabilities are refreshing. Updated routing instructions take effect on the next turn.",
      "info",
    );
}

export function getMcpSchemaMetrics(
  ctx?: PiContext,
): Record<string, number | string> {
  const snapshot = cachedSnapshots.get(cacheKey(ctx));
  const measurement = snapshot ? measureMcpCatalog(snapshot) : undefined;
  return {
    mode: "compiled",
    ...mcpSchemaMetrics,
    indexChars: measurement?.indexChars ?? 0,
    internalSchemaChars: measurement?.schemaChars ?? 0,
    reductionPercent: measurement
      ? Math.round(measurement.reductionRatio * 10_000) / 100
      : 0,
  };
}

/**
 * Machine-readable snapshot of the full MCP configuration + discovered catalogs,
 * for the .octocode/discovery.json inventory. Reads config fresh (cheap file
 * reads) but never spawns servers — tool lists come from the discovery cache.
 */
export async function getMcpDiscoverySnapshot(
  ctx?: PiContext,
): Promise<McpDiscoverySnapshot> {
  const loaded = await loadMcpConfig(ctx);
  const cached = new Map(
    (cachedCatalogs.get(cacheKey(ctx)) ?? []).map((entry) => [
      entry.name,
      entry,
    ]),
  );
  const servers: McpDiscoveryServer[] = [...loaded.servers.entries()].map(
    ([name, config]) => {
      const entry = cached.get(name);
      const tools = entry?.tools?.filter(isPlainRecord).map((tool) => ({
        name: String(tool["name"] ?? ""),
        description:
          typeof tool["description"] === "string"
            ? tool["description"]
            : "",
      }));
      return {
        name,
        command: config.url ?? config.command ?? "",
        args: config.args ?? [],
        ...(config.description ? { description: config.description } : {}),
        ...(tools ? { toolCount: tools.length, tools } : {}),
      };
    },
  );
  return { sources: loaded.sources, servers, warnings: loaded.warnings };
}

const DEFAULT_SERVER_ACTIONS = new Set<McpAction>([
  'describe', 'call', 'resources', 'read-resource', 'prompts', 'get-prompt', 'complete',
]);

function applyDefaultMcpServer(params: Record<string, unknown>): Record<string, unknown> {
  const action = params['action'] as McpAction | undefined;
  if (!action || params['server'] !== undefined || !DEFAULT_SERVER_ACTIONS.has(action)) return params;
  return { ...params, server: DEFAULT_OCTOCODE_MCP_SERVER_NAME };
}

export const __test__ = {
  applyDefaultMcpServer,
  registerMcpClientHandlers,
  persistMcpArtifacts,
  trackAsyncWork: trackMcpAsyncWork,
  setCachedMcpCatalog(
    ctx: PiContext | undefined,
    entries: ListedMcpServer[],
  ): void {
    cacheListedCatalog(ctx, entries);
  },
  clearCachedMcpCatalog(): void {
    cachedCatalogs.clear();
    cachedSnapshots.clear();
    cachedCatalogIndexes.clear();
    schemaCatalogs.clear();
    compiledValidators.clear();
    mcpSchemaMetrics.snapshotHits = 0;
    mcpSchemaMetrics.snapshotMisses = 0;
    mcpSchemaMetrics.blockedCalls = 0;
  },
};

/**
 * Per-query preflight: validate action-specific required fields before any
 * side-effecting MCP operation runs. Called for every query in the batch so
 * the entire batch is validated before the first action executes.
 */
export function preflightMcpQuery(query: QueryRecord): void {
  Object.assign(query, applyDefaultMcpServer(query));
  const action = query["action"] as McpAction | undefined;
  const server =
    typeof query["server"] === "string" && query["server"].length > 0
      ? query["server"]
      : undefined;
  const tool =
    typeof query["tool"] === "string" && query["tool"].trim().length > 0
      ? query["tool"]
      : undefined;
  const cfg = isPlainRecord(query["config"]) ? query["config"] : undefined;

  if (!action) {
    throw new Error(
      "MCP action is required; enabled tools are discovered automatically during extension initialization",
    );
  }
  if (action === "describe") {
    if (!server) throw new Error('describe requires server — use server:"octocode" for the built-in Octocode research server');
    if (!tool) throw new Error('describe requires tool — pass the exact MCP tool name, e.g. "localSearch" or "lspSearch"');
  } else if (action === "call") {
    if (!server) throw new Error('call requires server — use server:"octocode" for the built-in Octocode research server');
    if (!tool) throw new Error('call requires tool — pass the exact MCP tool name, e.g. "localSearch" or "lspSearch"');
  } else if (action === "resources" || action === "prompts") {
    if (!server) throw new Error(`${action} requires server`);
  } else if (action === "read-resource") {
    if (!server || typeof query["uri"] !== "string" || !query["uri"])
      throw new Error("read-resource requires server and uri");
  } else if (action === "get-prompt") {
    if (!server || typeof query["name"] !== "string" || !query["name"])
      throw new Error("get-prompt requires server and name");
  } else if (action === "complete") {
    if (
      !server ||
      !isPlainRecord(query["ref"]) ||
      !isPlainRecord(query["argument"])
    )
      throw new Error("complete requires server, ref, and argument");
  } else if (action === "restart") {
    if (!server) throw new Error("restart requires server");
  } else if (action === "enable" || action === "disable") {
    if (!server) throw new Error(`${action} requires server`);
  } else if (action === "add") {
    if (!server) throw new Error("add requires server");
    if (!cfg)
      throw new Error(
        "add requires a config object, e.g. {command, args, env, cwd}.",
      );
  } else if (action === "remove") {
    if (!server) throw new Error("remove requires server");
  }
}

function formatConfig(config: McpLoadedConfig, cwd = process.cwd()): string {
  const lines = ["Octocode MCP config"];
  lines.push(
    `servers: ${config.servers.size === 0 ? "none" : [...config.servers.keys()].join(", ")}`,
  );
  lines.push(
    `sources: ${config.sources.length === 0 ? "none" : config.sources.map((s) => `${s.scope}:${s.path}${s.trusted ? "" : " (untrusted)"}`).join("; ")}`,
  );
  if (config.warnings.length > 0)
    lines.push(`warnings:\n- ${config.warnings.join("\n- ")}`);
  lines.push(`canonical project path: ${projectMcpPath(cwd)}`);
  return lines.join("\n");
}

function formatMcpServerStatus(config: McpLoadedConfig): string {
  const running = [...connections.keys()];
  return [
    "Octocode MCP status",
    `configured: ${config.servers.size === 0 ? "none" : [...config.servers.keys()].join(", ")}`,
    `running: ${running.length === 0 ? "none" : running.join(", ")}`,
    config.warnings.length
      ? `warnings:\n- ${config.warnings.join("\n- ")}`
      : undefined,
  ]
    .filter(Boolean)
    .join("\n");
}


async function listServerTools(
  name: string,
  config: McpServerConfig,
  ctx: PiContext | undefined,
  signal: AbortSignal | undefined,
  timeoutMs?: number,
): Promise<ListedMcpServer> {
  const requestConfig =
    timeoutMs === undefined ? config : { ...config, timeoutMs };
  const connection = await ensureConnection(
    name,
    config,
    ctx,
    signal,
    timeoutMs,
  );
  const tools = await collectMcpPages<Record<string, unknown>>(
    `${name} tools/list`,
    async (cursor) =>
      connection.client.listTools(
        cursor ? { cursor } : undefined,
        requestOptions(requestConfig, signal),
      ) as Promise<McpCursorPage>,
    (page) =>
      Array.isArray((page as Record<string, unknown>)["tools"])
        ? ((page as Record<string, unknown>)["tools"] as Record<
            string,
            unknown
          >[])
        : [],
  );
  const instructions = [
    connection.client.getInstructions() ? `Server initialize instructions: ${connection.client.getInstructions()}` : '',
    config.instructions ? `User-configured instructions: ${config.instructions}` : '',
  ].filter(Boolean).join('\n');
  const lines = [`${name}: ${tools.length} tool(s)`];
  if (instructions)
    lines.push(`instructions: ${capCatalogText(instructions, 300)}`);
  for (const rawTool of tools) {
    const tool = rawTool as Record<string, unknown>;
    const description =
      typeof tool["description"] === "string" ? tool["description"] : "";
    lines.push(
      `- ${String(tool["name"])}: ${capCatalogText(description, 180)}${summarizeSchema(tool)}`,
    );
  }
  return {
    name,
    instructions,
    tools,
    text: lines.join("\n"),
    configSignature: configSignature(normalizeServerConfig(name, config)),
  };
}


async function ensureCurrentServerCatalog(
  server: string,
  loaded: McpLoadedConfig,
  ctx: PiContext | undefined,
  signal: AbortSignal | undefined,
): Promise<ListedMcpServer | undefined> {
  const config = loaded.servers.get(server);
  if (!config) return undefined;
  const signature = configSignature(normalizeServerConfig(server, config));
  const identity = { workspace: cacheKey(ctx), server, signature };
  const cached = cachedCatalogs
    .get(cacheKey(ctx))
    ?.find(
      (entry) => entry.name === server && entry.configSignature === signature,
    );
  if (cached && schemaCatalogs.isFresh(identity)) return cached;
  return schemaCatalogs.resolve(
    identity,
    () => listServerTools(server, config, ctx, signal),
    (listed) => {
      cacheListedCatalog(ctx, [listed], { loaded, updatePromptSnapshot: false });
    },
  );
}

function validatorForSchema(
  inputSchema: unknown,
  schemaDigest: string,
): McpCompiledSchemaValidator {
  const existing = compiledValidators.get(schemaDigest);
  if (existing) return existing;
  const validator = compileMcpSchemaValidator(inputSchema);
  compiledValidators.set(schemaDigest, validator);
  capMapSize(compiledValidators, 1_024);
  return validator;
}

async function validateOneMcpTool(
  target: { server: string; tool: string },
  loaded: McpLoadedConfig,
  ctx: PiContext | undefined,
  signal: AbortSignal | undefined,
): Promise<ValidatedMcpTool> {
  const imported = Boolean(
    loaded.configuredServers.get(target.server)?.discovered,
  );
  if (!isConfiguredMcpToolEnabled(loaded.configuredServers.get(target.server), target.tool)) throw new Error(`MCP tool is disabled by its source configuration: ${target.server}/${target.tool}`);
  const server = await ensureCurrentServerCatalog(
    target.server,
    loaded,
    ctx,
    signal,
  );
  if (!server) {
    const knownServers = (cachedCatalogs.get(cacheKey(ctx)) ?? []).map((s) => s.name);
    const serverHint = knownServers.length > 0
      ? ` Known servers: ${knownServers.join(', ')}.`
      : ' Check MCPTool action:"status" for connected servers.';
    throw new Error(`Unknown MCP server: "${target.server}".${serverHint}`);
  }
  const rawTool = server.tools.find(
    (candidate) =>
      isPlainRecord(candidate) && candidate["name"] === target.tool,
  );
  if (!isPlainRecord(rawTool) || !Object.hasOwn(rawTool, "inputSchema")) {
    const toolNames = server.tools.filter(isPlainRecord).map((t) => String(t["name"])).filter(Boolean);
    const toolHint = toolNames.length > 0 ? ` Available tools on "${target.server}": ${toolNames.join(', ')}.` : '';
    throw new Error(`Unknown MCP tool: ${target.server}/${target.tool}.${toolHint}`);
  }
  try {
    const enabled = getMcpEnablement(
      openOctocodeDb(),
      path.resolve(ctx?.cwd ?? process.cwd()),
      target.server,
      target.tool,
      true,
    );
    if (!enabled)
      throw new Error(`MCP tool is disabled: ${target.server}/${target.tool}`);
  } catch (error) {
    if ((error as Error).message.startsWith("MCP tool is disabled:"))
      throw error;
    // Managed definitions remain available if the DB is down. Imported tools fail closed.
    if (imported)
      throw new Error(`MCP tool is disabled: ${target.server}/${target.tool}`);
  }
  const inputSchema = rawTool["inputSchema"];
  const schemaDigest = stableSchemaDigest(inputSchema);
  const validator = validatorForSchema(inputSchema, schemaDigest);
  return {
    server: target.server,
    tool: target.tool,
    ...(server.instructions ? { instructions: server.instructions } : {}),
    inputSchema,
    schemaDigest,
    validator,
  };
}


export async function handleMcpAction(
  params: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext,
  options: { trustedBrowserAction?: boolean } = {},
): Promise<ToolCallResult> {
  const effectiveParams = applyDefaultMcpServer(params);
  if (isWorkerCapabilityClient()) return dispatchWorkerMcpAction(effectiveParams, signal);
  const action = effectiveParams["action"] as McpAction | undefined;
  if (!action)
    return result(
      "MCPTool action is required. Enabled MCP tools are discovered automatically during extension initialization.",
      undefined,
      true,
    );
  params = effectiveParams;
  const loaded = await loadMcpConfig(ctx);
  const serverName =
    typeof params["server"] === "string" ? params["server"] : undefined;

  if (action === 'list') {
    await refreshMcpCapabilities(ctx);
    const snapshot = getEffectiveMcpSnapshot(ctx) ?? snapshotFromListed(ctx, [], { loaded });
    const page = readMcpCatalogPage(snapshot, { offset: params['offset'] as number | undefined, textOffset: params['textOffset'] as number | undefined, limit: params['limit'] as number | undefined, catalogRevision: params['catalogRevision'] as string | undefined });
    return result(JSON.stringify(page), page, Boolean(page.diagnostic));
  }

  if (action === "config")
    return result(formatConfig(loaded, ctx?.cwd ?? process.cwd()), {
      sources: loaded.sources,
      warnings: loaded.warnings,
    });
  if (action === "status")
    return result(formatMcpServerStatus(loaded), {
      running: [...connections.keys()],
      warnings: loaded.warnings,
      schema: getMcpSchemaMetrics(ctx),
    });
  if (action === "stop") {
    const stopped = serverName
      ? await stopConnection(serverName)
      : stopAllMcpServers() > 0;
    if (serverName) invalidateServerCache(serverName);
    else invalidateCwdCache(ctx);
    return result(
      serverName
        ? `${serverName}: ${stopped ? "stopped" : "not running"}`
        : `stopped ${stopped ? "MCP servers" : "no MCP servers"}`,
    );
  }

  if (action === "enable" || action === "disable") {
    if (!serverName)
      return result(`MCPTool ${action} requires server`, undefined, true);
    const enabled = action === "enable";
    const scopeKey =
      params["scope"] === "global"
        ? "*"
        : path.resolve(ctx?.cwd ?? process.cwd());
    const toolName =
      typeof params["tool"] === "string" && params["tool"]
        ? params["tool"]
        : undefined;
    const db = openOctocodeDb();
    if (toolName)
      setMcpToolEnabled(db, scopeKey, serverName, toolName, enabled);
    else setMcpServerEnabled(db, scopeKey, serverName, enabled);
    await stopConnection(serverName);
    invalidateServerCache(serverName);
    invalidateCwdCache(ctx);
    markMcpPromptStale(ctx);
    void warmMcpCatalog(ctx);
    return result(
      `${serverName}${toolName ? `/${toolName}` : ""}: ${enabled ? "enabled" : "disabled"} (${scopeKey === "*" ? "global" : "workspace"})`,
    );
  }

  if (action === "add") {
    if (!serverName)
      return result("MCPTool add requires server", undefined, true);
    const scope: McpScope = params["scope"] === "global" ? "global" : "project";
    if (scope === "project") {
      const trusted = ctx?.isProjectTrusted
        ? Boolean(await ctx.isProjectTrusted())
        : false;
      if (!trusted)
        return result(
          'Refusing to write project MCP configuration: project trust could not be verified. Use scope:"global" or trust the project.',
          undefined,
          true,
        );
    }
    const cfg = isPlainRecord(params["config"]) ? params["config"] : undefined;
    if (!cfg)
      return result(
        "MCPTool add requires a config object, e.g. {command, args, env, cwd}.",
        undefined,
        true,
      );
    if (scope === "project" && !options.trustedBrowserAction) {
      // Project add writes the canonical project mcp.json and may spawn arbitrary code —
      // the same risk as global add. Require interactive consent; refuse non-interactively.
      const cmd2 = typeof cfg["command"] === "string" ? cfg["command"] : "?";
      const argText2 = Array.isArray(cfg["args"])
        ? (cfg["args"] as unknown[]).map(String).join(" ")
        : "";
      const choice2 = await runSelectOverlay(ctx, {
        title: `Add MCP server "${serverName}" to PROJECT servers.json? It will run locally as: ${cmd2}${argText2 ? " " + argText2 : ""}`,
        items: [
          {
            value: "deny",
            label: "Deny",
            description: "Do not modify the workspace-scoped global MCP config",
          },
          {
            value: "allow",
            label: "Allow",
            description:
              "Write the server config; it spawns on next MCPTool call",
          },
        ],
      });
      if (choice2 !== "allow") {
        const why2 =
          choice2 === undefined
            ? "no interactive UI to approve it"
            : "the user denied it";
        return result(
          `Project MCP add refused: ${why2}. Ask the user to edit the workspace-scoped config under $OCTOCODE_HOME/extension directly if they want this server.`,
          undefined,
          true,
        );
      }
    }
    if (scope === "global" && !options.trustedBrowserAction) {
      // Adding a server means spawning an arbitrary local process on the next
      // call — that decision belongs to the user, not the model. Hard gate:
      // interactive approval, or refuse when no UI is available.
      const cmd = typeof cfg["command"] === "string" ? cfg["command"] : "?";
      const argText = Array.isArray(cfg["args"])
        ? (cfg["args"] as unknown[]).map(String).join(" ")
        : "";
      const choice = await runSelectOverlay(ctx, {
        title:
          `Add MCP server "${serverName}" to GLOBAL servers.json? It will run locally as: ${cmd} ${argText}`.trim(),
        items: [
          {
            value: "deny",
            label: "Deny",
            description: "Do not modify $OCTOCODE_HOME/extension/mcp/servers.json",
          },
          {
            value: "allow",
            label: "Allow",
            description:
              "Write the server config; it spawns on next MCPTool call",
          },
        ],
      });
      if (choice !== "allow") {
        const why =
          choice === undefined
            ? "no interactive UI to approve it"
            : "the user denied it";
        return result(
          `Global MCP add refused: ${why}. Ask the user to edit $OCTOCODE_HOME/extension/mcp/servers.json directly if they want this server.`,
          undefined,
          true,
        );
      }
    }
    const target = scopeTargetPath(scope, ctx);
    let parsed: McpServerConfig;
    try {
      parsed = upsertServerInFile(target, serverName, cfg);
    } catch (error) {
      return result(
        `MCPTool add failed: ${(error as Error).message}`,
        undefined,
        true,
      );
    }
    // Apply immediately: drop any stale connection + cache so the next call spawns fresh.
    await stopConnection(serverName);
    invalidateServerCache(serverName);
    invalidateCwdCache(ctx);
    markMcpPromptStale(ctx);
    void warmMcpCatalog(ctx);
    const shadowNote =
      serverName === DEFAULT_OCTOCODE_MCP_SERVER_NAME
        ? " (overrides the built-in octocode default — env defaults for full-text + npm cache are still merged in)"
        : "";
    return result(
      `${serverName}: added to ${scope} mcp.json (${target}) as \`${parsed.command}${parsed.args?.length ? " " + parsed.args.join(" ") : ""}\`.${shadowNote} Active on next MCPTool call — no agent restart needed.`,
    );
  }

  if (action === "remove") {
    if (!serverName)
      return result("MCPTool remove requires server", undefined, true);
    if (serverName === DEFAULT_OCTOCODE_MCP_SERVER_NAME) {
      return result(
        `"${DEFAULT_OCTOCODE_MCP_SERVER_NAME}" is the built-in default MCP server (pinned local octocode-mcp with an npx fallback) and cannot be removed. You may override its config with action:add, or stop the live process with action:stop.`,
        undefined,
        true,
      );
    }
    const scope: McpScope = params["scope"] === "global" ? "global" : "project";
    if (scope === "project") {
      const trusted = ctx?.isProjectTrusted
        ? Boolean(await ctx.isProjectTrusted())
        : false;
      if (!trusted)
        return result(
          "Refusing to write project MCP configuration: project trust could not be verified.",
          undefined,
          true,
        );
    }
    if (scope === "project" && !options.trustedBrowserAction) {
      // Require interactive consent before removing from project config.
      const rmChoice = await runSelectOverlay(ctx, {
        title: `Remove MCP server "${serverName}" from PROJECT servers.json?`,
        items: [
          {
            value: "deny",
            label: "Deny",
            description:
              "Keep the server in the workspace-scoped global MCP config",
          },
          {
            value: "allow",
            label: "Allow",
            description:
              "Remove it from the workspace-scoped global MCP config",
          },
        ],
      });
      if (rmChoice !== "allow") {
        const rmWhy =
          rmChoice === undefined
            ? "no interactive UI to approve it"
            : "the user denied it";
        return result(
          `Project MCP remove refused: ${rmWhy}. Ask the user to edit the workspace-scoped config under $OCTOCODE_HOME/extension directly if they want to remove this server.`,
          undefined,
          true,
        );
      }
    }
    const target = scopeTargetPath(scope, ctx);
    let removed: boolean;
    try {
      removed = removeServerFromFile(target, serverName);
    } catch (error) {
      return result(
        `MCPTool remove failed: ${(error as Error).message}`,
        undefined,
        true,
      );
    }
    await stopConnection(serverName);
    const removedConfig = loaded.configuredServers.get(serverName);
    if (removed && removedConfig?.auth === "oauth" && removedConfig.url) {
      await revokeStoredMcpOAuthCredentials(
        serverName,
        removedConfig.url,
      ).catch(() => undefined);
    }
    invalidateServerCache(serverName);
    invalidateCwdCache(ctx);
    markMcpPromptStale(ctx);
    void warmMcpCatalog(ctx);
    const note =
      serverName === DEFAULT_OCTOCODE_MCP_SERVER_NAME
        ? " (note: the built-in octocode default re-appears unless overridden)"
        : "";
    return result(
      removed
        ? `${serverName}: removed from ${scope} mcp.json (${target}).${note}`
        : `${serverName}: not present in ${scope} mcp.json (${target}).${note}`,
      undefined,
      !removed,
    );
  }

  if (serverName && !loaded.servers.has(serverName)) {
    return result(
      `Unknown MCP server: ${serverName}\nConfigured: ${[...loaded.servers.keys()].join(", ") || "none"}`,
      loaded,
      true,
    );
  }

  if (action === "restart") {
    if (!serverName)
      return result("mcp restart requires server", undefined, true);
    await stopConnection(serverName);
    invalidateServerCache(serverName);
    await ensureConnection(
      serverName,
      loaded.servers.get(serverName)!,
      ctx,
      signal,
    );
    markMcpPromptStale(ctx);
    void warmMcpCatalog(ctx);
    return result(
      `${serverName}: restarted; execution catalog is refreshing (start /new to refresh model routing)`,
    );
  }

  if (action === "describe") {
    if (loaded.servers.size === 0)
      return result(formatConfig(loaded, ctx?.cwd ?? process.cwd()));
    const toolName =
      typeof params["tool"] === "string" ? params["tool"] : undefined;
    const server = await ensureCurrentServerCatalog(
      serverName!,
      loaded,
      ctx,
      signal,
    );
    if (!server)
      return result(`Unknown MCP server: ${serverName}`, undefined, true);
    const tool = server.tools.find(
      (candidate) => isPlainRecord(candidate) && candidate["name"] === toolName,
    );
    if (!tool)
      return result(
        `Unknown MCP tool: ${serverName}/${toolName}`,
        { server, warnings: loaded.warnings },
        true,
      );
    return result(
      stringify({
        server: server.name,
        instructions: server.instructions,
        tool,
      }),
      {
        server: server.name,
        instructions: server.instructions,
        tool,
        warnings: loaded.warnings,
      },
    );
  }

  if (
    action === "resources" ||
    action === "read-resource" ||
    action === "prompts" ||
    action === "get-prompt" ||
    action === "complete"
  ) {
    if (!serverName)
      return result(`MCPTool ${action} requires server`, undefined, true);
    const config = loaded.servers.get(serverName)!;
    const connection = await ensureConnection(serverName, config, ctx, signal);
    let payload: unknown;
    if (action === "resources") {
      const [resources, resourceTemplates] = await Promise.all([
        collectMcpPages<unknown>(
          `${serverName} resources/list`,
          async (cursor) =>
            connection.client.listResources(
              cursor ? { cursor } : undefined,
              requestOptions(config, signal),
            ) as Promise<McpCursorPage>,
          (page) =>
            Array.isArray((page as Record<string, unknown>)["resources"])
              ? ((page as Record<string, unknown>)["resources"] as unknown[])
              : [],
        ),
        collectMcpPages<unknown>(
          `${serverName} resources/templates/list`,
          async (cursor) =>
            connection.client.listResourceTemplates(
              cursor ? { cursor } : undefined,
              requestOptions(config, signal),
            ) as Promise<McpCursorPage>,
          (page) =>
            Array.isArray(
              (page as Record<string, unknown>)["resourceTemplates"],
            )
              ? ((page as Record<string, unknown>)[
                  "resourceTemplates"
                ] as unknown[])
              : [],
        ),
      ]);
      payload = { resources, resourceTemplates };
    } else if (action === "read-resource") {
      const uri = typeof params["uri"] === "string" ? params["uri"] : "";
      if (!uri)
        return result("MCPTool read-resource requires uri", undefined, true);
      payload = await connection.client.readResource(
        { uri },
        requestOptions(config, signal),
      );
    } else if (action === "prompts") {
      const prompts = await collectMcpPages<unknown>(
        `${serverName} prompts/list`,
        async (cursor) =>
          connection.client.listPrompts(
            cursor ? { cursor } : undefined,
            requestOptions(config, signal),
          ) as Promise<McpCursorPage>,
        (page) =>
          Array.isArray((page as Record<string, unknown>)["prompts"])
            ? ((page as Record<string, unknown>)["prompts"] as unknown[])
            : [],
      );
      payload = { prompts };
    } else if (action === "get-prompt") {
      const name = typeof params["name"] === "string" ? params["name"] : "";
      if (!name)
        return result("MCPTool get-prompt requires name", undefined, true);
      payload = await connection.client.getPrompt(
        {
          name,
          arguments: isPlainRecord(params["arguments"])
            ? (params["arguments"] as Record<string, string>)
            : undefined,
        },
        requestOptions(config, signal),
      );
    } else {
      if (!isPlainRecord(params["ref"]) || !isPlainRecord(params["argument"]))
        return result(
          "MCPTool complete requires ref and argument objects",
          undefined,
          true,
        );
      payload = await connection.client.complete(
        { ref: params["ref"] as never, argument: params["argument"] as never },
        requestOptions(config, signal),
      );
    }
    return result(stringify(payload), payload);
  }

  if (action === "call") {
    if (!serverName)
      return result("MCPTool call requires server", undefined, true);
    const tool = params["tool"];
    if (typeof tool !== "string" || tool.trim().length === 0)
      return result("MCPTool call requires tool", undefined, true);
    const config = loaded.servers.get(serverName)!;
    const argumentsPayload = isPlainRecord(params["arguments"])
      ? params["arguments"]
      : {};
    let validated: ValidatedMcpTool;
    try {
      validated = await validateOneMcpTool(
        { server: serverName, tool },
        loaded,
        ctx,
        signal,
      );
    } catch (error) {
      const code =
        error instanceof McpSchemaUnsupportedError
          ? error.code
          : "SCHEMA_UNAVAILABLE";
      return result(
        `${code} ${serverName}/${tool}\n${(error as Error).message}`,
        { server: serverName, tool },
        true,
      );
    }
    const expectedSchemaDigest = params["__expectedSchemaDigest"];
    if (
      typeof expectedSchemaDigest === "string" &&
      expectedSchemaDigest !== validated.schemaDigest
    ) {
      mcpSchemaMetrics.blockedCalls += 1;
      return result(
        `MCP_SCHEMA_STALE ${serverName}/${tool}\nThe active Pi proxy was compiled for a previous schema revision. Run MCPTool action:"describe" again.`,
        {
          server: serverName,
          tool,
          expectedSchemaDigest,
          currentSchemaDigest: validated.schemaDigest,
        },
        true,
      );
    }
    const validation = validated.validator.validate(argumentsPayload);
    if (!validation.valid) {
      mcpSchemaMetrics.blockedCalls += 1;
      return result(
        `MCP_SCHEMA_INVALID ${serverName}/${tool}\n${formatMcpSchemaValidationErrors(validation.errors, { server: serverName, tool })}`,
        {
          server: serverName,
          tool,
          inputSchema: validated.inputSchema,
          errors: validation.errors,
        },
        true,
      );
    }
    const connection = await ensureConnection(serverName, config, ctx, signal);
    const payload = await connection.client.callTool(
      { name: tool, arguments: argumentsPayload },
      requestOptions(config, signal),
    );
    // Only content delivered by a successful query establishes read state.
    // A mixed batch can succeed overall while individual reads fail or are absent.
    if (
      serverName === DEFAULT_OCTOCODE_MCP_SERVER_NAME &&
      tool === "localFetch" &&
      payload?.isError !== true
    ) {
      const cwd = ctx?.cwd ?? process.cwd();
      const queries = Array.isArray(argumentsPayload["queries"])
        ? argumentsPayload["queries"]
        : [];
      const structured = payload?.structuredContent;
      const readResults = isPlainRecord(structured) && Array.isArray(structured["results"])
        ? structured["results"]
        : [];
      await Promise.all(
        readResults.map(async (entry: unknown) => {
          if (!isPlainRecord(entry) || entry["status"] !== undefined) return;
          const index = entry["index"];
          const data = entry["data"];
          if (
            typeof index !== "number" || !Number.isInteger(index) || index < 0 ||
            !isPlainRecord(data) || typeof data["content"] !== "string"
          ) return;
          const q: unknown = queries[index];
          const p = isPlainRecord(q) ? q["path"] : undefined;
          if (typeof p === "string" && p.trim().length > 0) {
            await recordFileReadState(p, cwd).catch(() => undefined);
          }
        }),
      );
    }
    const tableContent =
      params["responseView"] === "table"
        ? resolveMcpCallTable(payload)
        : undefined;
    return {
      content: tableContent ?? resolveMcpCallContent(payload),
      details: summarizeMcpCallDetails(payload),
      isError: payload?.isError === true,
    };
  }

  return result(`Unknown MCP action: ${action}`, undefined, true);
}

export function registerMcpTool(
  pi: PiInstance,
  registeredToolNames: Set<string>,
  registerFn: (
    pi: PiInstance,
    registeredToolNames: Set<string>,
    toolDefinition: ToolDefinition,
  ) => void,
): void {
  const proxyState: DynamicMcpProxyState = {
    supported:
      typeof pi.registerTool === "function" &&
      typeof pi.getActiveTools === "function" &&
      typeof pi.getAllTools === "function" &&
      typeof pi.setActiveTools === "function",
    describedSchemas: new Map(),
    bindingsByName: new Map(),
    latestByIdentity: new Map(),
    unavailableNames: new Set(),
  };
  dynamicMcpProxyStates.set(pi, proxyState);
  if (typeof pi.on === "function") {
    pi.on("session_start", async () => {
      proxyState.describedSchemas.clear();
      proxyState.latestByIdentity.clear();
      const active = pi.getActiveTools?.();
      if (active && proxyState.bindingsByName.size > 0) {
        pi.setActiveTools?.(active.filter(name => !proxyState.bindingsByName.has(name)));
      }
    });
    pi.on("session_compact", async () => {
      const active = new Set(pi.getActiveTools?.() ?? []);
      for (const [identity] of proxyState.describedSchemas) {
        const visible = proxyState.latestByIdentity.get(identity);
        if (!visible || !active.has(visible.name)) {
          proxyState.describedSchemas.delete(identity);
        }
      }
    });
  }

  // ── Per-query item schema: each queries[] entry carries one MCP action + fields. ──
  const itemSchema = mcpGatewayItemSchema();

  // Universal ordered queries[] envelope: all queries are preflighted before the first side-effect.
  const parameters = buildQueryEnvelopeSchema(itemSchema, { allowParallel: true });

  const execute = async (
    toolCallId: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
    onUpdate?: unknown,
    ctx?: PiContext,
  ): Promise<ToolCallResult> => {
    setManagedStatus(ctx, MCP_STATUS_NAME, "mcp · running");
    const parallelServers = new Set<string>();
    try {
      const output = await executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate:
          typeof onUpdate === "function"
            ? (onUpdate as (u: ToolCallResult) => void)
            : undefined,
        ctx,
        passthroughSingle: true,
        allowParallel: true,
        preflight(query) {
          preflightMcpQuery(query);
          if (proxyState.supported && query["action"] === "call") {
            const server = String(query["server"] ?? "");
            const tool = String(query["tool"] ?? "");
            const workerView = isWorkerCapabilityClient()
              ? getCurrentWorkerCapabilities()
              : undefined;
            const granted = !workerView || workerView.snapshot.mcpTools.some(
              candidate => candidate.server === server && candidate.tool === tool,
            );
            const describedDigest = proxyState.describedSchemas.get(
              mcpToolIdentity(server, tool),
            );
            if (granted && !describedDigest) {
              throw new Error(schemaRequiredMessage(server, tool));
            }
          }
          if (params["queryRunType"] !== "parallel") return;
          const action = query["action"] as McpAction;
          if (
            ![
              "status",
              "describe",
              "call",
              "resources",
              "read-resource",
              "prompts",
              "get-prompt",
              "complete",
            ].includes(action)
          ) {
            throw new Error(
              `parallel MCP batches do not support the mutating ${action} action`,
            );
          }
          const server = typeof query["server"] === "string" ? query["server"] : undefined;
          if (server && parallelServers.has(server)) {
            throw new Error(`parallel MCP batches require distinct servers; batch same-server ${server} queries inside the target tool arguments`);
          }
          if (server) parallelServers.add(server);
        },
        async execute(
          query,
          _index,
          _itemId,
          batchSignal,
          _onItemUpdate,
          itemCtx,
        ) {
          const action = query["action"] as McpAction;
          const identity = action === "call"
            ? mcpToolIdentity(String(query["server"] ?? ""), String(query["tool"] ?? ""))
            : undefined;
          const expectedSchemaDigest = identity
            ? proxyState.describedSchemas.get(identity)
            : undefined;
          const actionResult = await handleMcpAction(
            expectedSchemaDigest ? { ...query, __expectedSchemaDigest: expectedSchemaDigest } : query,
            batchSignal,
            itemCtx,
          );
          return action === "describe"
            ? activateDescribedMcpProxy(pi, proxyState, actionResult, itemCtx)
            : actionResult;
        },
        summarize: summarizeMcpBatchResult,
      });
      if (isWorkerCapabilityClient() && output.isError) throw new Error(output.content.filter(part => part.type === 'text').map(part => part.text).join('\n'));
      return output;
    } catch (error) {
      if (isWorkerCapabilityClient()) throw error;
      if (error instanceof QueryBatchError && error.completedCount > 0) throw error;
      return result(`[MCP_ERROR] ${(error as Error).message}`, undefined, true);
    } finally {
      setManagedStatus(ctx, MCP_STATUS_NAME, undefined);
    }
  };

  const common = {
    label: "MCPTool",
    description: DIRECT_TOOL_DESCRIPTIONS.MCPTool!,
    promptSnippet: "Gateway to MCP servers and exact-schema loader. Built-in octocode catalog in <mcp_catalog_index>.",
    promptGuidelines: [
      `Select from <mcp_catalog_index>; omit server for Octocode. Describe only when the target schema is not active, then call the activated Pi tool. Generic action:call requires that same described schema.`,
      "Batch independent target queries inside arguments.queries[]. Use outer parallel execution only across different servers.",
      "add/remove writes mcp.json; restart/stop manages connections. Do not add untrusted MCP config without user approval.",
    ],
    parameters,
    execute,
    renderCall: renderMcpCall,
    renderResult: renderMcpResult,
  } satisfies Omit<ToolDefinition, "name">;

  registerFn(pi, registeredToolNames, { name: "MCPTool", ...common });
}
