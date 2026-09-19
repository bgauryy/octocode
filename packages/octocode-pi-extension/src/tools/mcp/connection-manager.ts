import {
  Client,
  StreamableHTTPClientTransport,
  type Transport,
} from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import type { PiContext } from '../../types.js';
import { DEFAULT_MCP_RETRY_DELAY_MS as DEFAULT_RETRY_DELAY_MS, DEFAULT_MCP_STARTUP_RETRIES as DEFAULT_STARTUP_RETRIES, MAX_MCP_RETRY_DELAY_MS as MAX_RETRY_DELAY_MS } from '../../contracts/mcp-connection-policy.js';
import { readOwnVersion } from '../../package-metadata.js';
import { assertPathAllowed } from '../path-guard.js';
import { registerMcpClientHandlers } from './client-handlers.js';
import {
  buildServerEnv,
  buildServerHeaders,
  configSignature,
  normalizeServerConfig,
  requestOptions,
  resolveServerCwd,
  type McpServerConfig,
} from './config.js';
import { createMcpOAuthFlow, type McpOAuthFlow } from './oauth.js';
import type { McpConnection, McpConnectionHealth } from './types.js';

export interface McpConnectionManager {
  ensure(
    name: string,
    config: McpServerConfig,
    ctx?: PiContext,
    signal?: AbortSignal,
    timeoutMs?: number,
  ): Promise<McpConnection>;
  stop(name: string): Promise<boolean>;
  stopAll(): number;
  isConnected(name: string): boolean;
  connectedNames(): string[];
  probeConnected(signal?: AbortSignal, timeoutMs?: number): Promise<McpConnectionHealth[]>;
  configSignatures(): Map<string, string>;
}

function abortReason(signal: AbortSignal): Error {
  return signal.reason instanceof Error ? signal.reason : new Error('MCP connection startup aborted');
}

function waitForRetry(delayMs: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.reject(abortReason(signal));
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, delayMs);
    const onAbort = () => {
      clearTimeout(timer);
      reject(abortReason(signal!));
    };
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

/** Run one initial startup plus a bounded number of cancellable retry attempts. */
export async function retryMcpStartup<T>(
  attempt: () => Promise<T>,
  retries = DEFAULT_STARTUP_RETRIES,
  retryDelayMs = DEFAULT_RETRY_DELAY_MS,
  signal?: AbortSignal,
): Promise<T> {
  let failure: unknown;
  for (let index = 0; index <= retries; index += 1) {
    if (signal?.aborted) throw abortReason(signal);
    try {
      return await attempt();
    } catch (error) {
      failure = error;
      if (signal?.aborted) throw abortReason(signal);
      if (index >= retries) throw error;
      const delay = Math.min(retryDelayMs * (2 ** index), MAX_RETRY_DELAY_MS);
      await waitForRetry(delay, signal);
    }
  }
  throw failure;
}

export function createMcpConnectionManager(options: {
  onCatalogChanged(name: string, ctx?: PiContext): void;
  onClientInvalidated(name: string, ctx?: PiContext): void;
  onServerInvalidated(name: string): void;
  trackAsyncWork<T>(work: Promise<T>): Promise<T>;
}): McpConnectionManager {
  const connections = new Map<string, McpConnection>();
  const pending = new Map<string, Promise<McpConnection>>();
  const pendingControllers = new Map<string, AbortController>();

  async function closeConnection(name: string): Promise<boolean> {
    const connection = connections.get(name);
    if (!connection) return false;
    connections.delete(name);
    connection.oauth?.close();
    await connection.client.close().catch(() => undefined);
    return true;
  }

  async function stop(name: string): Promise<boolean> {
    const pendingController = pendingControllers.get(name);
    pendingController?.abort(new Error(`MCP server ${name} stopped`));
    const closed = await closeConnection(name);
    return closed || Boolean(pendingController);
  }

  async function connectOnce(
    name: string,
    config: McpServerConfig,
    signature: string,
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

    if (config.transport === 'http' || config.url) {
      if (config.auth === 'oauth') oauth = await createMcpOAuthFlow(name, config.url!, ctx);
      transport = new StreamableHTTPClientTransport(new URL(config.url!), {
        requestInit: { headers: buildServerHeaders(config) },
        ...(oauth ? { authProvider: oauth.provider } : {}),
      });
      if (oauth) oauth.attachTransport(transport as StreamableHTTPClientTransport);
    } else {
      const cwd = resolveServerCwd(config, ctx);
      assertPathAllowed(cwd, ctx?.cwd ?? process.cwd(), `mcp:${name}`);
      const stdio = new StdioClientTransport({
        command: config.command!,
        args: config.args ?? [],
        cwd,
        env: buildServerEnv(name, config),
        stderr: 'pipe',
      });
      transport = stdio;
      stderr = stdio.stderr;
    }

    const client = new Client(
      { name: 'octocode-pi-extension', version: readOwnVersion() ?? 'unknown' },
      {
        capabilities: {
          roots: { listChanged: true },
          sampling: {},
          elicitation: { form: {}, url: {} },
        },
        inputRequired: { autoFulfill: true, maxRounds: 8 },
        versionNegotiation: { mode: 'auto' },
        listChanged: {
          tools: { onChanged: () => options.onCatalogChanged(name, ctx) },
          prompts: { onChanged: () => options.onCatalogChanged(name, ctx) },
          resources: { onChanged: () => options.onCatalogChanged(name, ctx) },
        },
      },
    );
    registerMcpClientHandlers(client, name, ctx, () => options.onClientInvalidated(name, ctx));

    const startedAt = Date.now();
    const connection: McpConnection = {
      name,
      config,
      configSig: signature,
      client,
      transport,
      stderr: [],
      startedAt,
      health: { name, status: 'healthy', checkedAt: startedAt, startedAt },
      ...(oauth ? { oauth } : {}),
    };
    stderr?.on('data', (chunk: Buffer) => {
      const text = chunk.toString('utf8').trim();
      if (!text) return;
      connection.stderr.push(text);
      while (connection.stderr.length > 20) connection.stderr.shift();
    });
    transport.onclose = () => {
      if (connections.get(name) === connection) connections.delete(name);
      connection.health = {
        name,
        status: 'unhealthy',
        checkedAt: Date.now(),
        startedAt,
        error: 'transport closed',
      };
      connection.oauth?.close();
    };
    transport.onerror = error => {
      connection.stderr.push(error.message);
      while (connection.stderr.length > 20) connection.stderr.shift();
      connection.health = {
        name,
        status: 'unhealthy',
        checkedAt: Date.now(),
        startedAt,
        error: error.message.slice(0, 500),
      };
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
      const stderrText = connection.stderr.length > 0
        ? `\nstderr:\n${connection.stderr.join('\n')}`
        : '';
      await client.close().catch(() => undefined);
      const authorized = oauth && !oauthRetry
        ? await oauth.hasTokens().catch(() => false)
        : false;
      oauth?.close();
      if (authorized) return connectOnce(name, config, signature, ctx, signal, true, timeoutMs);
      throw new Error(`${(error as Error).message}${stderrText}`);
    }

    connections.set(name, connection);
    return connection;
  }

  async function ensure(
    name: string,
    rawConfig: McpServerConfig,
    ctx?: PiContext,
    signal?: AbortSignal,
    timeoutMs?: number,
  ): Promise<McpConnection> {
    const config = normalizeServerConfig(name, rawConfig);
    const signature = configSignature(config);
    const existing = connections.get(name);
    if (existing) {
      if (existing.configSig === signature) return existing;
      // Config drift retires only the established connection. A concurrent
      // startup keeps its own signature fence below and must not be mistaken
      // for an explicit user/session stop.
      await closeConnection(name);
      options.onServerInvalidated(name);
    }

    const inFlight = pending.get(name);
    if (inFlight) {
      const connection = await inFlight;
      if (connection.configSig === signature) return connection;
    }

    const controller = new AbortController();
    const startupSignal = signal
      ? AbortSignal.any([signal, controller.signal])
      : controller.signal;
    const work = retryMcpStartup(
      () => connectOnce(name, config, signature, ctx, startupSignal, false, timeoutMs),
      config.startupRetries ?? DEFAULT_STARTUP_RETRIES,
      config.retryDelayMs ?? DEFAULT_RETRY_DELAY_MS,
      startupSignal,
    );
    pending.set(name, work);
    pendingControllers.set(name, controller);
    try {
      return await work;
    } finally {
      if (pending.get(name) === work) pending.delete(name);
      if (pendingControllers.get(name) === controller) pendingControllers.delete(name);
    }
  }

  async function probeConnected(signal?: AbortSignal, timeoutMs = 2_000): Promise<McpConnectionHealth[]> {
    return Promise.all([...connections.entries()].map(async ([name, connection]) => {
      const started = Date.now();
      try {
        await connection.client.ping(requestOptions({ ...connection.config, timeoutMs }, signal));
        connection.health = {
          name,
          status: 'healthy',
          checkedAt: Date.now(),
          startedAt: connection.startedAt,
          latencyMs: Date.now() - started,
        };
      } catch (error) {
        // A cancelled status request says nothing about server health and must
        // not evict a connection that the next turn can still use.
        if (signal?.aborted) throw abortReason(signal);
        const failedHealth: McpConnectionHealth = {
          name,
          status: 'unhealthy',
          checkedAt: Date.now(),
          startedAt: connection.startedAt,
          error: String((error as Error)?.message ?? error).slice(0, 500),
        };
        if (connections.get(name) === connection) connections.delete(name);
        connection.oauth?.close();
        await connection.client.close().catch(() => undefined);
        connection.health = failedHealth;
        options.onClientInvalidated(name);
      }
      return { ...connection.health };
    }));
  }

  return {
    ensure,
    stop,
    stopAll() {
      const names = [...connections.keys()];
      for (const name of names) {
        const connection = connections.get(name);
        connections.delete(name);
        connection?.oauth?.close();
        if (connection) options.trackAsyncWork(connection.client.close().catch(() => undefined));
      }
      // Preserve the historical non-blocking shutdown contract: callers that
      // own an in-flight ensure() retain its result/error. Request cancellation
      // and targeted stop(name) still abort retry waits deterministically.
      pending.clear();
      pendingControllers.clear();
      return names.length;
    },
    isConnected: name => connections.has(name),
    connectedNames: () => [...connections.keys()],
    probeConnected,
    configSignatures: () => new Map(
      [...connections].map(([name, connection]) => [name, connection.configSig]),
    ),
  };
}
