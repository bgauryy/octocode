import {
  Client,
  StreamableHTTPClientTransport,
  type Transport,
} from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import type { PiContext } from '../../types.js';
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
import type { McpConnection } from './types.js';

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
  configSignatures(): Map<string, string>;
}

export function createMcpConnectionManager(options: {
  onCatalogChanged(name: string, ctx?: PiContext): void;
  onClientInvalidated(name: string, ctx?: PiContext): void;
  onServerInvalidated(name: string): void;
  trackAsyncWork<T>(work: Promise<T>): Promise<T>;
}): McpConnectionManager {
  const connections = new Map<string, McpConnection>();
  const pending = new Map<string, Promise<McpConnection>>();

  async function stop(name: string): Promise<boolean> {
    const connection = connections.get(name);
    if (!connection) return false;
    connections.delete(name);
    connection.oauth?.close();
    await connection.client.close().catch(() => undefined);
    return true;
  }

  async function connect(
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

    const connection: McpConnection = {
      name,
      config,
      configSig: signature,
      client,
      transport,
      stderr: [],
      startedAt: Date.now(),
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
      connection.oauth?.close();
    };
    transport.onerror = error => connection.stderr.push(error.message);

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
      if (authorized) return connect(name, config, signature, ctx, signal, true, timeoutMs);
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
      await stop(name);
      options.onServerInvalidated(name);
    }

    const inFlight = pending.get(name);
    if (inFlight) {
      const connection = await inFlight;
      if (connection.configSig === signature) return connection;
    }

    const work = connect(name, config, signature, ctx, signal, false, timeoutMs);
    pending.set(name, work);
    try {
      return await work;
    } finally {
      if (pending.get(name) === work) pending.delete(name);
    }
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
      pending.clear();
      return names.length;
    },
    isConnected: name => connections.has(name),
    connectedNames: () => [...connections.keys()],
    configSignatures: () => new Map(
      [...connections].map(([name, connection]) => [name, connection.configSig]),
    ),
  };
}
