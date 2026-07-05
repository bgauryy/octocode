import { Octokit } from 'octokit';
import { throttling } from '@octokit/plugin-throttling';
import type { OctokitOptions } from '@octokit/core';
import { createHash } from 'crypto';
import { getGitHubToken } from '../serverConfig.js';
import { getServerConfig } from '../serverConfig.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { version } from '../../package.json';
import { recordRateLimit } from '../session.js';

function hashToken(token: string): string {
  return createHash('sha256').update(token).digest('hex').substring(0, 16);
}

export const OctokitWithThrottling = Octokit.plugin(throttling);

const TOKEN_TTL_MS = 5 * 60 * 1000;

const MAX_INSTANCES = 50;

const PURGE_INTERVAL_MS = 60 * 1000;

interface CachedInstance {
  client: InstanceType<typeof OctokitWithThrottling>;
  createdAt: number;
}

function isExpired(cached: CachedInstance): boolean {
  return Date.now() - cached.createdAt > TOKEN_TTL_MS;
}

const instances = new Map<string, CachedInstance>();

let purgeTimer: ReturnType<typeof setInterval> | null = null;

function purgeExpiredInstances(): void {
  for (const [key, cached] of instances.entries()) {
    if (isExpired(cached)) {
      instances.delete(key);
    }
  }

  if (instances.size > MAX_INSTANCES) {
    // Map is insertion-ordered — oldest entries are first.
    const excess = instances.size - MAX_INSTANCES;
    let evicted = 0;
    for (const [key] of instances) {
      if (evicted >= excess) break;
      instances.delete(key);
      evicted++;
    }
  }
}

function ensurePurgeTimer(): void {
  if (purgeTimer) return;
  purgeTimer = setInterval(purgeExpiredInstances, PURGE_INTERVAL_MS);
  if (typeof purgeTimer === 'object' && 'unref' in purgeTimer) {
    purgeTimer.unref();
  }
}

const MAX_RATE_LIMIT_RETRIES = 3;

const MAX_RETRY_AFTER_SECONDS = 60;

function recordThrottleRateLimit(
  limitType: 'primary' | 'secondary',
  retryAfter: number,
  options: { method: string; url: string }
): void {
  recordRateLimit({
    limit_type: limitType,
    retry_after_seconds: retryAfter,
    api_method: options.method,
    api_url: options.url,
    provider: 'github',
  });
}

const createThrottleOptions = () => ({
  onRateLimit: (
    retryAfter: number,
    options: { method: string; url: string },
    _octokit: unknown,
    retryCount: number
  ) => {
    recordThrottleRateLimit('primary', retryAfter, options);

    return (
      retryCount < MAX_RATE_LIMIT_RETRIES &&
      retryAfter < MAX_RETRY_AFTER_SECONDS
    );
  },
  onSecondaryRateLimit: (
    retryAfter: number,
    options: { method: string; url: string },
    _octokit: unknown,
    retryCount: number
  ) => {
    recordThrottleRateLimit('secondary', retryAfter, options);

    return (
      retryCount < MAX_RATE_LIMIT_RETRIES &&
      retryAfter < MAX_RETRY_AFTER_SECONDS
    );
  },
});

function createOctokitInstance(
  token?: string
): InstanceType<typeof OctokitWithThrottling> {
  const config = getServerConfig();
  const baseUrl = config.githubApiUrl;

  const quietLog = {
    debug: () => {},
    info: () => {},
    warn: () => {},
    error: (...args: unknown[]) => console.error(...args),
  };

  const options: OctokitOptions & {
    throttle: ReturnType<typeof createThrottleOptions>;
  } = {
    userAgent: `octocode-mcp/${version}`,
    baseUrl,
    request: { timeout: config.timeout || 30000, log: quietLog },
    throttle: createThrottleOptions(),
    log: quietLog,
    ...(token && { auth: token }),
  };

  return new OctokitWithThrottling(options);
}

export async function getOctokit(
  authInfo?: AuthInfo
): Promise<InstanceType<typeof OctokitWithThrottling>> {
  ensurePurgeTimer();

  const token = authInfo?.token ?? (await getGitHubToken());
  const key = token ? hashToken(token) : 'ANONYMOUS';

  const cached = instances.get(key);
  if (cached && !isExpired(cached)) {
    return cached.client;
  }

  if (instances.size >= MAX_INSTANCES) {
    purgeExpiredInstances();
  }

  const newInstance = createOctokitInstance(token ?? undefined);
  instances.set(key, { client: newInstance, createdAt: Date.now() });
  return newInstance;
}

export const MAX_BRANCH_CACHE_SIZE = 200;

const defaultBranchCache = new Map<string, string>();

export async function resolveDefaultBranch(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<string> {
  const cacheKey = `${owner}/${repo}`;
  const cached = defaultBranchCache.get(cacheKey);
  if (cached) return cached;

  const octokit = await getOctokit(authInfo);

  try {
    const { data } = await octokit.rest.repos.get({ owner, repo });
    const branch = data.default_branch;
    cacheDefaultBranch(cacheKey, branch);
    return branch;
  } catch {
    void 0;
  }

  const candidates = ['main', 'master'] as const;
  for (const candidate of candidates) {
    try {
      await octokit.rest.repos.getBranch({ owner, repo, branch: candidate });
      cacheDefaultBranch(cacheKey, candidate);
      return candidate;
    } catch {
      void 0;
    }
  }

  throw new Error(
    `Could not determine default branch for ${owner}/${repo}. ` +
      `The repository may not exist, require authentication, or be inaccessible.`
  );
}

function cacheDefaultBranch(cacheKey: string, branch: string): void {
  if (defaultBranchCache.size >= MAX_BRANCH_CACHE_SIZE) {
    const oldest = defaultBranchCache.keys().next().value;
    if (oldest !== undefined) defaultBranchCache.delete(oldest);
  }
  defaultBranchCache.set(cacheKey, branch);
}

export function clearOctokitInstances(): void {
  instances.clear();
  defaultBranchCache.clear();

  if (purgeTimer) {
    clearInterval(purgeTimer);
    purgeTimer = null;
  }
}
