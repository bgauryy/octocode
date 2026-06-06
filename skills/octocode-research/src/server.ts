import express, { type Express, type Request, type Response, type NextFunction } from 'express';
import type { Server } from 'http';
import { writeFileSync, unlinkSync, mkdirSync } from 'fs';
import { join } from 'path';
import { homedir } from 'os';
import { toolsRoutes } from './routes/tools.js';
import { promptsRoutes } from './routes/prompts.js';
import { errorHandler } from './middleware/errorHandler.js';
import { requestLogger } from './middleware/logger.js';
import { initializeProviders, initializeSession, logSessionInit } from './index.js';
import { initializeMcpContent, isMcpInitialized } from './mcpCache.js';
import { getLogsPath, initializeLogger } from './utils/logger.js';
import { getAllCircuitStates, clearAllCircuits, stopCircuitCleanup } from './utils/circuitBreaker.js';
import { agentLog, successLog, errorLog, dimLog, warnLog } from './utils/colors.js';
import { fireAndForgetWithTimeout } from './utils/asyncTimeout.js';
import { errorQueue } from './utils/errorQueue.js';

declare const __PACKAGE_VERSION__: string;

const HOST = process.env.OCTOCODE_RESEARCH_HOST || 'localhost';

const getPort = (raw?: string): number => {
  const DEFAULT_PORT = 1987;
  if (!raw) return DEFAULT_PORT;

  const port = Number(raw);

  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error(
      `Invalid OCTOCODE_RESEARCH_PORT: "${raw}". ` +
      `Please provide an integer between 1024 and 65535.`
    );
  }

  return port;
};

const PORT = getPort(process.env.OCTOCODE_RESEARCH_PORT || process.env.OCTOCODE_PORT);

const MAX_IDLE_TIME_MS = 30 * 60 * 1000;
const IDLE_CHECK_INTERVAL_MS = 120 * 1000;

let server: Server | null = null;
let lastRequestTime: number = Date.now();
let idleCheckInterval: NodeJS.Timeout | null = null;
let isShuttingDown = false;

const OCTOCODE_DIR = process.env.OCTOCODE_HOME || join(homedir(), '.octocode');
export const PID_FILE = join(OCTOCODE_DIR, `research-server-${PORT}.pid`);

function writePidFile(): void {
  try {
    mkdirSync(OCTOCODE_DIR, { recursive: true, mode: 0o700 });
    writeFileSync(PID_FILE, String(process.pid), { mode: 0o600 });
  } catch {
    void 0;
  }
}

function removePidFile(): void {
  try {
    unlinkSync(PID_FILE);
  } catch {
    void 0;
  }
}


function checkIdleRestart(): void {
  const idleTime = Date.now() - lastRequestTime;
  const idleSeconds = Math.floor(idleTime / 1000);
  
  if (idleTime > MAX_IDLE_TIME_MS) {
    console.log(warnLog(`⚠️ Server idle for ${idleSeconds}s (>${MAX_IDLE_TIME_MS / 1000}s). Initiating restart...`));
    gracefulShutdown('IDLE_TIMEOUT');
  } else if (idleTime > MAX_IDLE_TIME_MS / 2) {
    console.log(dimLog(`⏰ Idle: ${idleSeconds}s / ${MAX_IDLE_TIME_MS / 1000}s`));
  }
}


function startIdleCheck(): void {
  if (idleCheckInterval) return;
  
  idleCheckInterval = setInterval(checkIdleRestart, IDLE_CHECK_INTERVAL_MS);
  
  idleCheckInterval.unref();
  
  console.log(dimLog(`⏱️ Idle check started (${IDLE_CHECK_INTERVAL_MS / 1000}s interval, ${MAX_IDLE_TIME_MS / 1000}s threshold)`));
}


function stopIdleCheck(): void {
  if (idleCheckInterval) {
    clearInterval(idleCheckInterval);
    idleCheckInterval = null;
    console.log(successLog('✅ Idle check interval stopped'));
  }
}

export async function createServer(): Promise<Express> {
  initializeLogger();
  
  initializeSession();

  const app = express();
  app.use(express.json());
  
  app.use((_req: Request, _res: Response, next: NextFunction) => {
    lastRequestTime = Date.now();
    next();
  });

  app.use(requestLogger);
  
  app.get('/health', (_req: Request, res: Response) => {
    const memoryUsage = process.memoryUsage();
    const recentErrors = errorQueue.getRecent(5);
    const initialized = isMcpInitialized();
    const idleTimeMs = Date.now() - lastRequestTime;

    res.json({
      status: initialized ? 'ok' : 'initializing',
      host: HOST,
      port: PORT,
      version: __PACKAGE_VERSION__,
      uptime: Math.floor(process.uptime()),
      processManager: 'self (detached daemon)',
      pid: process.pid,
      idle: {
        currentMs: idleTimeMs,
        thresholdMs: MAX_IDLE_TIME_MS,
        checkIntervalMs: IDLE_CHECK_INTERVAL_MS,
        percentToRestart: Math.round((idleTimeMs / MAX_IDLE_TIME_MS) * 100),
      },
      memory: {
        heapUsed: Math.round(memoryUsage.heapUsed / 1024 / 1024),
        heapTotal: Math.round(memoryUsage.heapTotal / 1024 / 1024),
        rss: Math.round(memoryUsage.rss / 1024 / 1024),
      },
      circuits: getAllCircuitStates(),
      errors: {
        queueSize: errorQueue.size,
        recentErrors: recentErrors.map((e) => ({
          timestamp: e.timestamp.toISOString(),
          context: e.context,
          message: e.error.message,
        })),
      },
    });
  });
  
  app.use('/tools', toolsRoutes);
  app.use('/prompts', promptsRoutes);

  app.use((_req: Request, res: Response) => {
    res.status(404).json({
      success: false,
      error: {
        message: 'Route not found',
        code: 'NOT_FOUND',
        availableRoutes: [
          'GET  /health',
          'GET  /tools/initContext',
          'GET  /tools/list',
          'GET  /tools/info',
          'GET  /tools/info/:toolName',
          'GET  /tools/metadata',
          'GET  /tools/schemas',
          'GET  /tools/system',
          'POST /tools/call/:toolName',
          'GET  /prompts/list',
          'GET  /prompts/info/:promptName',
        ],
        hint: 'All tools are called via POST /tools/call/{toolName}',
      },
    });
  });

  app.use(errorHandler);
  
  return app;
}


function gracefulShutdown(signal: string): void {
  if (isShuttingDown) {
    console.log(dimLog(`Already shutting down, ignoring ${signal}`));
    return;
  }
  isShuttingDown = true;

  console.log(agentLog(`\n🛑 Received ${signal}. Starting graceful shutdown...`));

  const FORCE_EXIT_TIMEOUT_MS = 30 * 1000;
  setTimeout(() => {
    console.log(warnLog('⚠️ Force exiting due to drain timeout'));
    process.exit(1);
  }, FORCE_EXIT_TIMEOUT_MS).unref();

  removePidFile();

  stopIdleCheck();

  stopCircuitCleanup();
  console.log(successLog('✅ Circuit cleanup interval stopped'));

  clearAllCircuits();
  console.log(successLog('✅ Circuit breakers cleared'));
  
  if (server) {
    console.log(dimLog('⏳ Waiting for connections to drain...'));
    server.close((err) => {
      if (err) {
        console.error(errorLog('❌ Error closing server:'), err);
        process.exit(1);
      }
      console.log(successLog('✅ HTTP server closed'));
      process.exit(0);
    });
  } else {
    process.exit(0);
  }
}

export async function startServer(): Promise<void> {
  const app = await createServer();
  
  await new Promise<void>((resolve) => {
    const httpServer = app.listen(PORT, HOST, () => {
      server = httpServer;
      writePidFile();
      console.log(agentLog(`🔍 Octocode Research Server running on http://${HOST}:${PORT} (pid: ${process.pid})`));
      console.log(dimLog(`⏳ initializing context...`));
      
      initializeMcpContent()
        .then(() => initializeProviders())
        .then(() => {
          console.log(successLog('✅ Context initialized - Server Ready'));
          
          lastRequestTime = Date.now();
          
          startIdleCheck();
          
          console.log(agentLog(`📁 Logs: ${getLogsPath()}`));
          console.log(agentLog(`\nRoutes:`));
          console.log(dimLog(`  GET  /health                  - Server health`));
          console.log(dimLog(`  GET  /tools/initContext       - System prompt + schemas (LOAD FIRST)`));
          console.log(dimLog(`  GET  /tools/system            - System prompt only`));
          console.log(dimLog(`  GET  /tools/list              - List all tools`));
          console.log(dimLog(`  GET  /tools/info              - All tools with details`));
          console.log(dimLog(`  GET  /tools/info/:toolName    - Tool schema (BEFORE calling)`));
          console.log(dimLog(`  GET  /tools/metadata          - Raw MCP metadata`));
          console.log(dimLog(`  GET  /tools/schemas           - All tools schemas`));
          console.log(dimLog(`  POST /tools/call/:toolName    - Execute tool`));
          console.log(dimLog(`  GET  /prompts/list            - List prompts`));
          console.log(dimLog(`  GET  /prompts/info/:name      - Get prompt content`));

          fireAndForgetWithTimeout(
            () => logSessionInit(),
            5000,
            'logSessionInit'
          );
        })
        .catch((err) => {
          console.error(errorLog('❌ Initialization failed:'), err);
        });

      resolve();
    });
  });
}

process.on('SIGTERM', () => gracefulShutdown('SIGTERM'));
process.on('SIGINT', () => gracefulShutdown('SIGINT'));

const isMainModule = import.meta.url === `file://${process.argv[1]}`;
if (isMainModule) {
  startServer().catch((err) => {
    console.error(errorLog('❌ Failed to start server:'), err);
    process.exit(1);
  });
}
