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
  isLoggingEnabled,
  initializeProviders,
  clearProviderCache,
  initializeSession,
  logSessionInit,
  logSessionError,
  logToolCall,
  loadToolContent,
  STARTUP_ERRORS,
  startCacheGC,
  stopCacheGC,
  completeMetadata,
  getOctocodeDir,
  configureSecurity,
  securityRegistry,
} from '@octocodeai/octocode-tools-core';
import { createLogger, LoggerFactory, Logger } from './utils/core/logger.js';
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

function createShutdownHandler(
  server: McpServer,
  getLogger: () => Logger | null,
  state: ShutdownState
) {
  return async (signal?: string) => {
    if (state.inProgress) return;
    state.inProgress = true;

    try {
      const logger = getLogger();

      await logger?.info('Shutting down', { signal });

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
  gracefulShutdown: (signal?: string) => Promise<void>,
  getLogger: () => Logger | null
) {
  process.once('SIGINT', () => gracefulShutdown('SIGINT'));
  process.once('SIGTERM', () => gracefulShutdown('SIGTERM'));

  process.stdin.once('close', () => gracefulShutdown('STDIN_CLOSE'));

  process.once('uncaughtException', error => {
    getLogger()?.error('Uncaught exception', { error: error.message });
    logSessionError('startup', STARTUP_ERRORS.UNCAUGHT_EXCEPTION.code).catch(
      () => {
        /* Session log failure is non-fatal during crash handling */
      }
    );
    gracefulShutdown('UNCAUGHT_EXCEPTION');
  });

  process.once('unhandledRejection', reason => {
    getLogger()?.error('Unhandled rejection', { reason: String(reason) });
    logSessionError('startup', STARTUP_ERRORS.UNHANDLED_REJECTION.code).catch(
      () => {
        /* Session log failure is non-fatal during crash handling */
      }
    );
    gracefulShutdown('UNHANDLED_REJECTION');
  });
}

export async function registerAllTools(server: McpServer) {
  const logger = LoggerFactory.getLogger(server, 'tools');
  const activeProvider = getActiveProvider();

  if (activeProvider === 'github') {
    const token = await getGitHubToken();
    if (!token) {
      await logger.warning('No GitHub token - limited functionality');
      process.stderr.write(
        '⚠️  No GitHub token available - some features may be limited\n'
      );
    } else {
      await logger.info('GitHub token ready');
    }
  }

  const { registerTools } = await import('./tools/toolsManager.js');
  const { successCount, failedTools, failedToolErrors } =
    await registerTools(server);
  await logger.info('Tools registered', { count: successCount });

  if (failedTools.length > 0) {
    await logger.warning('Some tools failed to register', {
      failedCount: failedTools.length,
      failedTools,
      failedToolErrors,
    });
  }

  if (successCount === 0) {
    await logSessionError('startup', STARTUP_ERRORS.NO_TOOLS_REGISTERED.code);
    await logger.error('Tool registration failed');
    throw new Error(STARTUP_ERRORS.NO_TOOLS_REGISTERED.message);
  }
}

async function createServer(): Promise<McpServer> {
  const capabilities: {
    tools: { listChanged: boolean };
    logging: Record<string, never>;
  } = {
    tools: { listChanged: false },
    logging: {},
  };

  return new McpServer(SERVER_CONFIG, {
    capabilities,
    instructions: completeMetadata.instructions,
  });
}

async function startServer() {
  const shutdownState: ShutdownState = { inProgress: false, timeout: null };
  let logger: Logger | null = null;

  const getLogger = () => logger;

  try {
    await initialize();
    configureSecurity({
      logToolCall,
      logSessionError,
      isLoggingEnabled,
    });
    securityRegistry.addAllowedRoots([getOctocodeDir()]);
    await initializeProviders();
    await loadToolContent();
    const session = initializeSession();

    const server = await createServer();
    await registerAllTools(server);

    const gracefulShutdown = createShutdownHandler(
      server,
      getLogger,
      shutdownState
    );
    setupProcessHandlers(gracefulShutdown, getLogger);

    const transport = new StdioServerTransport();
    await server.connect(transport);

    logger = createLogger(server, 'server');

    await logger.info('Server ready', {
      pid: process.pid,
      sessionId: session.getSessionId(),
      provider: getActiveProvider(),
    });

    if (isCloneEnabled()) {
      startCacheGC(getOctocodeDir());
    }

    logSessionInit().catch(() => {
      /* Background session init log failure is non-fatal */
    });
  } catch (startupError) {
    await logger?.error('Startup failed', { error: String(startupError) });
    await logSessionError('startup', STARTUP_ERRORS.STARTUP_FAILED.code);
    process.exit(1);
  }
}

startServer().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error || 'Unknown error');
  process.stderr.write(`❌ Startup failed: ${message}\n`);
  process.exit(1);
});
