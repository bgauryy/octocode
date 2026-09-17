import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import { McpServer, Implementation } from '@modelcontextprotocol/server';
import { buildMcpInstructions } from '@octocodeai/octocode-core/mcp';
import type { McpToolConfig } from './tools/toolConfig.js';
import { bootRuntime } from './native/select.js';
import {
  clearAllCache,
  clearOctokitInstances,
  initialize,
  cleanup,
  getGitHubToken,
  getActiveProvider,
  initializeProviders,
  clearProviderCache,
  STARTUP_ERRORS,
  startCacheGC,
  stopCacheGC,
  getOctocodeDir,
  configureSecurity,
  securityRegistry,
  getGrammarCapabilities,
} from '@octocodeai/octocode-tools-core';
import { version, name } from '../package.json';

interface ShutdownState {
  inProgress: boolean;
  timeout: ReturnType<typeof setTimeout> | null;
}

const SERVER_CONFIG: Implementation = {
  name: `${name}_${version}`,
  title: 'Octocode MCP',
  version,
};

const SHUTDOWN_TIMEOUT_MS = 5000;

function createShutdownHandler(server: McpServer, state: ShutdownState) {
  return async (_signal?: string) => {
    if (state.inProgress) return;
    state.inProgress = true;

    try {
      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }

      state.timeout = setTimeout(() => process.exit(1), SHUTDOWN_TIMEOUT_MS);

      stopCacheGC();
      clearAllCache();
      clearOctokitInstances();
      clearProviderCache();
      cleanup();

      try {
        await server.close();
      } catch {
        // server.close() failure is non-fatal during shutdown
      }

      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }

      process.exit(0);
    } catch (error) {
      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }
      const message =
        error instanceof Error ? error.message : String(error ?? 'unknown');
      process.stderr.write(`Shutdown error: ${message}\n`);
      process.exit(1);
    }
  };
}

function setupProcessHandlers(
  gracefulShutdown: (signal?: string) => Promise<void>
) {
  process.once('SIGINT', () => gracefulShutdown('SIGINT'));
  process.once('SIGTERM', () => gracefulShutdown('SIGTERM'));

  process.stdin.once('close', () => gracefulShutdown('STDIN_CLOSE'));

  process.once('uncaughtException', _error => {
    gracefulShutdown('UNCAUGHT_EXCEPTION');
  });

  process.once('unhandledRejection', _reason => {
    gracefulShutdown('UNHANDLED_REJECTION');
  });
}

export async function registerAllTools(
  server: McpServer,
  enabledTools?: McpToolConfig[]
) {
  const activeProvider = getActiveProvider();

  if (activeProvider === 'github') {
    // Pre-flight: validate a GitHub token is resolvable at startup rather than failing on first tool call.
    await getGitHubToken();
  }

  const { registerTools } = await import('./tools/toolsManager.js');
  const { successCount, failedTools, failedToolErrors } = enabledTools
    ? await registerTools(server, undefined, { enabledTools })
    : await registerTools(server);

  if (failedTools.length > 0) {
    const details = Object.entries(failedToolErrors ?? {})
      .map(([n, err]) => `  - ${n}: ${err}`)
      .join('\n');
    process.stderr.write(
      `Warning: ${failedTools.length} tool(s) failed to register:\n${details}\n`
    );
  }

  if (successCount === 0) {
    throw new Error(STARTUP_ERRORS.NO_TOOLS_REGISTERED.message);
  }
}

async function createServer(enabledTools: McpToolConfig[]): Promise<McpServer> {
  const enabledNames = enabledTools.map(tool => tool.name);
  const grammarCapabilities = enabledNames.includes('astSearch')
    ? getGrammarCapabilities()
    : undefined;
  const capabilities: {
    tools: { listChanged: boolean };
  } = {
    tools: { listChanged: false },
  };

  return new McpServer(SERVER_CONFIG, {
    capabilities,
    instructions: buildMcpInstructions(
      enabledNames,
      grammarCapabilities ? { grammarCapabilities } : {}
    ),
  });
}

async function startToolsCoreServer() {
  const shutdownState: ShutdownState = { inProgress: false, timeout: null };

  await initialize();
  configureSecurity({});
  securityRegistry.addAllowedRoots([getOctocodeDir()]);
  await initializeProviders();

  const { getEnabledTools } = await import('./tools/toolsManager.js');
  const enabledTools = await getEnabledTools();
  const server = await createServer(enabledTools);
  await registerAllTools(server, enabledTools);

  const gracefulShutdown = createShutdownHandler(server, shutdownState);
  setupProcessHandlers(gracefulShutdown);

  const transport = new StdioServerTransport();
  await server.connect(transport);

  startCacheGC(getOctocodeDir());
}

async function startServer() {
  try {
    // Native is strictly opt-in (OCTOCODE_RUNTIME=native + a resolvable addon)
    // and falls back to tools-core on any failure. Default stays tools-core.
    await bootRuntime({
      startToolsCore: startToolsCoreServer,
      startNative: async () => {
        const { startNativeMcp } = await import('./native/index.mjs');
        await startNativeMcp();
      },
    });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : String(error ?? 'unknown');
    process.stderr.write(`Server initialization failed: ${message}\n`);
    process.exit(1);
  }
}

startServer().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error || 'Unknown error');
  process.stderr.write(`❌ Startup failed: ${message}\n`);
  process.exit(1);
});
