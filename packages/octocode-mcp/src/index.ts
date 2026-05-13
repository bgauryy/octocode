import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { Implementation } from '@modelcontextprotocol/sdk/types.js';

import { registerPrompts } from './prompts/prompts.js';
import { clearAllCache } from './utils/http/cache.js';
import { clearOctokitInstances } from './github/client.js';
import {
  initialize,
  cleanup,
  getGitHubToken,
  arePromptsEnabled,
  isCloneEnabled,
  getActiveProvider,
  isLoggingEnabled,
} from './serverConfig.js';
import {
  initializeProviders,
  clearProviderCache,
} from './providers/factory.js';
import { createLogger, LoggerFactory, Logger } from './utils/core/logger.js';
import {
  initializeSession,
  logSessionInit,
  logSessionError,
  logToolCall,
} from './session.js';
import { loadToolContent } from './tools/toolMetadata/state.js';
import type { CompleteMetadata } from '@octocodeai/octocode-core';
import { version, name } from '../package.json';
import { STARTUP_ERRORS } from './errors/domainErrors.js';
import { startCacheGC, stopCacheGC } from './tools/github_clone_repo/cache.js';
import { getOctocodeDir } from 'octocode-shared';
import { configureSecurity } from './utils/securityBridge.js';
import { securityRegistry } from 'octocode-security-utils';
import { isLocalTool } from './tools/toolNames.js';

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
        // server.close() may throw if already closed; shutdown still proceeds to exit.
      }

      if (state.timeout) {
        clearTimeout(state.timeout);
        state.timeout = null;
      }

      process.exit(0);
    } catch {
      // Graceful shutdown failed; still clear timeout and exit with error.
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

export async function registerAllTools(
  server: McpServer,
  _content: CompleteMetadata
) {
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

  // Dynamic import: defers all tool schema construction until AFTER metadata
  // is loaded. Zod .describe() captures strings eagerly at call time, so
  // schema modules must be evaluated after the metadata singleton is set.
  const { registerTools } = await import('./tools/toolsManager.js');
  const { successCount, failedTools } = await registerTools(server);
  await logger.info('Tools registered', { count: successCount });

  if (failedTools.length > 0) {
    await logger.warning('Some tools failed to register', {
      failedCount: failedTools.length,
      failedTools,
    });
  }

  if (successCount === 0) {
    await logSessionError('startup', STARTUP_ERRORS.NO_TOOLS_REGISTERED.code);
    await logger.error('Tool registration failed');
    throw new Error(STARTUP_ERRORS.NO_TOOLS_REGISTERED.message);
  }
}

// Server Initialization

async function createServer(content: CompleteMetadata): Promise<McpServer> {
  const capabilities: {
    prompts?: Record<string, never>;
    tools: { listChanged: boolean };
    logging: Record<string, never>;
  } = {
    tools: { listChanged: false },
    logging: {},
  };

  if (arePromptsEnabled()) {
    capabilities.prompts = {};
  }

  const genericHints = [
    'Every query must include a unique id; match responses via results[].id',
    "Follow 'mainResearchGoal', 'researchGoal', 'reasoning', 'hints' to navigate research",
    'Do findings answer your question? If partial, identify gaps and continue',
    'Got 3+ examples? Consider stopping to avoid over-research',
    'Check last modified dates - skip stale content',
    'Try broader terms or related concepts when results are empty',
    'Remove filters one at a time to find what blocks results',
    'Separate concerns into multiple simpler queries',
    'If stuck in loop - STOP and ask user',
    'If LSP tools return text-based fallback, install typescript-language-server for semantic analysis',
  ].join('\n');

  const fullInstructions = content.instructions
    ? `${content.instructions}\n\n${genericHints}`
    : genericHints;

  return new McpServer(SERVER_CONFIG, {
    capabilities,
    instructions: fullInstructions,
  });
}

async function startServer() {
  const shutdownState: ShutdownState = { inProgress: false, timeout: null };
  let logger: Logger | null = null;

  // Lazy getter: shutdown/error handlers always get the current logger
  // (null before connect, valid after connect, works during the server lifetime)
  const getLogger = () => logger;

  try {
    // Phase 1: Initialize configuration & providers
    await initialize();
    configureSecurity({
      logToolCall,
      logSessionError,
      isLoggingEnabled,
      isLocalTool,
    });
    securityRegistry.addAllowedRoots([getOctocodeDir()]);
    await initializeProviders();
    const content = await loadToolContent();
    const session = initializeSession();

    // Phase 2: Create server, register tools & prompts (pre-connect)
    const server = await createServer(content);
    await registerAllTools(server, content);
    if (arePromptsEnabled()) {
      registerPrompts(server, content);
    }

    // Phase 3: Setup shutdown/crash handlers BEFORE connect
    // Uses lazy getLogger() so handlers work both with and without a logger
    const gracefulShutdown = createShutdownHandler(
      server,
      getLogger,
      shutdownState
    );
    setupProcessHandlers(gracefulShutdown, getLogger);

    // Phase 4: Connect transport — server is now live on stdio
    const transport = new StdioServerTransport();
    await server.connect(transport);

    // Phase 5: Logger works NOW (transport connected, isConnected() = true)
    logger = createLogger(server, 'server');

    await logger.info('Server ready', {
      pid: process.pid,
      sessionId: session.getSessionId(),
      provider: getActiveProvider(),
    });

    // Start periodic cache GC when clone support is enabled
    if (isCloneEnabled()) {
      startCacheGC(getOctocodeDir());
    }

    // Background session logging
    logSessionInit().catch(() => {
      /* Background session init log failure is non-fatal */
    });
  } catch (startupError) {
    await logger?.error('Startup failed', { error: String(startupError) });
    await logSessionError('startup', STARTUP_ERRORS.STARTUP_FAILED.code);
    process.exit(1);
  }
}

// Entry Point

startServer().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error || 'Unknown error');
  process.stderr.write(`❌ Startup failed: ${message}\n`);
  process.exit(1);
});
