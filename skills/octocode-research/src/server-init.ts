import { spawn } from 'child_process';
import { join } from 'path';


const PORT = parseInt(process.env.OCTOCODE_RESEARCH_PORT || process.env.OCTOCODE_PORT || '1987', 10);
const HOST = process.env.OCTOCODE_RESEARCH_HOST || 'localhost';
const HEALTH_URL = `http://${HOST}:${PORT}/health`;
const MAX_WAIT_MS = parseInt(process.env.OCTOCODE_INIT_TIMEOUT || '30000', 10);
const POLL_INTERVAL_MS = parseInt(process.env.OCTOCODE_POLL_INTERVAL || '500', 10);

interface HealthResponse {
  status: 'ok' | 'initializing' | string;
}


async function checkHealth(): Promise<HealthResponse | null> {
  try {
    const response = await fetch(HEALTH_URL, {
      signal: AbortSignal.timeout(5000),
    });

    if (!response.ok) {
      return null;
    }

    return (await response.json()) as HealthResponse;
  } catch (error: unknown) {
    if (error instanceof Error && !error.message.includes('ECONNREFUSED')) {
      console.error(`[server-init] Health check error: ${error.message}`);
    }
    return null;
  }
}


function startServer(): Promise<void> {
  return new Promise((resolve, reject) => {
    const scriptDir = new URL('.', import.meta.url).pathname;
    const serverScript = join(scriptDir, 'server.js');

    const child = spawn('node', [serverScript], {
      stdio: 'ignore',
      cwd: scriptDir,
      detached: true,
    });

    child.on('error', (err) => {
      console.error(`[server-init] Failed to start server: ${err.message}`);
      reject(err);
    });

    child.unref();

    setTimeout(() => {
      console.log(`[server-init] Spawned detached server process (pid: ${child.pid})`);
      resolve();
    }, 100);
  });
}


async function waitForReady(): Promise<boolean> {
  const startTime = Date.now();
  let pollInterval = POLL_INTERVAL_MS;

  while (Date.now() - startTime < MAX_WAIT_MS) {
    const health = await checkHealth();

    if (health?.status === 'ok') {
      return true;
    }

    if (health?.status === 'initializing') {
      console.log('[server-init] Server initializing...');
    }

    await new Promise((resolve) => setTimeout(resolve, pollInterval));
    pollInterval = Math.min(pollInterval * 1.5, 2000);
  }

  return false;
}


async function main(): Promise<void> {
  const initialHealth = await checkHealth();

  if (initialHealth?.status === 'ok') {
    console.log('ok');
    process.exit(0);
  }

  if (initialHealth?.status === 'initializing') {
    console.log('[server-init] Server is initializing, waiting...');
    const ready = await waitForReady();
    if (ready) {
      console.log('ok');
      process.exit(0);
    } else {
      console.error('[server-init] ERROR: Server stuck in initializing state');
      process.exit(1);
    }
  }

  console.log('[server-init] Server not running, starting detached daemon...');

  try {
    await startServer();
  } catch {
    console.error('[server-init] ERROR: Failed to spawn server process');
    process.exit(1);
  }

  const ready = await waitForReady();
  if (!ready) {
    console.error('[server-init] ERROR: Server failed to start within timeout');
    process.exit(1);
  }

  console.log('ok');
  process.exit(0);
}

main();
