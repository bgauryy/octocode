import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { Implementation } from '@modelcontextprotocol/sdk/types.js';

import {
  clearAllCache,
  clearOctokitInstances,
  initialize,
  cleanup,
  getGitHubToken,
  isCloneEnabled,
  getActiveProvider,
  initializeProviders,
  clearProviderCache,
  loadToolContent,
  STARTUP_ERRORS,
  startCacheGC,
  stopCacheGC,
  completeMetadata,
  getOctocodeDir,
  configureSecurity,
  securityRegistry,
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
  return async (signal?: string) => {
    if (state.inProgress) return;
    state.inProgress = true;

    try {
      void signal;

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
        void 0;
      }

      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }

      process.exit(0);
    } catch {
      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }
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

export async function registerAllTools(server: McpServer) {
  const activeProvider = getActiveProvider();

  if (activeProvider === 'github') {
    const token = await getGitHubToken();
    void token;
  }

  const { registerTools } = await import('./tools/toolsManager.js');
  const { successCount, failedTools, failedToolErrors } =
    await registerTools(server);

  void failedTools;
  void failedToolErrors;

  if (successCount === 0) {
    throw new Error(STARTUP_ERRORS.NO_TOOLS_REGISTERED.message);
  }
}

async function createServer(): Promise<McpServer> {
  const capabilities: {
    tools: { listChanged: boolean };
  } = {
    tools: { listChanged: false },
  };

  return new McpServer(SERVER_CONFIG, {
    capabilities,
    instructions: completeMetadata.systemPrompt,
  });
}

async function startServer() {
  const shutdownState: ShutdownState = { inProgress: false, timeout: null };

  try {
    await initialize();
    configureSecurity({});
    securityRegistry.addAllowedRoots([getOctocodeDir()]);
    await initializeProviders();
    await loadToolContent();

    const server = await createServer();
    await registerAllTools(server);

    const gracefulShutdown = createShutdownHandler(server, shutdownState);
    setupProcessHandlers(gracefulShutdown);

    const transport = new StdioServerTransport();
    await server.connect(transport);

    if (isCloneEnabled()) {
      startCacheGC(getOctocodeDir());
    }
  } catch {
    process.exit(1);
  }
}

startServer().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error || 'Unknown error');
  process.stderr.write(`❌ Startup failed: ${message}\n`);
  process.exit(1);
});
