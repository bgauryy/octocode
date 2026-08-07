import { execSync, exec } from 'node:child_process';
import { promisify } from 'node:util';

const execAsync = promisify(exec);

export interface NodeEnvironmentStatus {
  nodeInstalled: boolean;
  nodeVersion: string | null;
  npmInstalled: boolean;
  npmVersion: string | null;
  registryStatus: 'ok' | 'slow' | 'failed';
  registryLatency: number | null;
  octocodePackageAvailable: boolean;
  octocodePackageVersion: string | null;
}

const REGISTRY_OK_THRESHOLD = 1000;
const REGISTRY_SLOW_THRESHOLD = 3000;
const CHECK_TIMEOUT = 4000;

export function checkNodeInPath(): {
  installed: boolean;
  version: string | null;
} {
  try {
    const version = execSync('node --version', {
      encoding: 'utf-8',
      timeout: 5000,
      stdio: ['pipe', 'pipe', 'pipe'],
    }).trim();
    return { installed: true, version };
  } catch {
    return { installed: false, version: null };
  }
}

export function checkNpmInPath(): {
  installed: boolean;
  version: string | null;
} {
  try {
    const version = execSync('npm --version', {
      encoding: 'utf-8',
      timeout: 5000,
      stdio: ['pipe', 'pipe', 'pipe'],
    }).trim();
    return { installed: true, version: `v${version}` };
  } catch {
    return { installed: false, version: null };
  }
}

export async function checkNpmRegistry(): Promise<{
  status: 'ok' | 'slow' | 'failed';
  latency: number | null;
}> {
  const registryUrl = 'https://registry.npmjs.org';

  try {
    const start = Date.now();

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), CHECK_TIMEOUT);

    const response = await fetch(registryUrl, {
      method: 'HEAD',
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    const latency = Date.now() - start;

    if (!response.ok) {
      return { status: 'failed', latency };
    }

    if (latency > REGISTRY_SLOW_THRESHOLD) {
      return { status: 'slow', latency };
    }

    if (latency > REGISTRY_OK_THRESHOLD) {
      return { status: 'slow', latency };
    }

    return { status: 'ok', latency };
  } catch {
    return { status: 'failed', latency: null };
  }
}

export async function checkOctocodePackageAsync(): Promise<{
  available: boolean;
  version: string | null;
}> {
  try {
    const { stdout } = await execAsync('npm view octocode-mcp version', {
      timeout: CHECK_TIMEOUT,
    });
    return { available: true, version: stdout.trim() };
  } catch {
    return { available: false, version: null };
  }
}

export async function checkNodeEnvironment(): Promise<NodeEnvironmentStatus> {
  const nodeCheck = checkNodeInPath();
  const npmCheck = checkNpmInPath();
  const registryCheck = await checkNpmRegistry();
  const octocodeCheck = await checkOctocodePackageAsync();

  return {
    nodeInstalled: nodeCheck.installed,
    nodeVersion: nodeCheck.version,
    npmInstalled: npmCheck.installed,
    npmVersion: npmCheck.version,
    registryStatus: registryCheck.status,
    registryLatency: registryCheck.latency,
    octocodePackageAvailable: octocodeCheck.available,
    octocodePackageVersion: octocodeCheck.version,
  };
}
