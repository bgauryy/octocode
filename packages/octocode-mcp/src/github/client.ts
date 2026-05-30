import { Octokit } from 'octokit';
import { throttling } from '@octokit/plugin-throttling';
import type { OctokitOptions } from '@octokit/core';
import { createHash } from 'crypto';
import { getGitHubToken } from '../serverConfig.js';
import { getServerConfig } from '../serverConfig.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { version } from '../../package.json';
import { logRateLimit } from '../session.js';

/**
 * Hash a token for use as a Map key.
 * Prevents raw tokens from appearing in memory dumps or debug output.
 */
function hashToken(token: string): string {
  return createHash('sha256').update(token).digest('hex').substring(0, 16);
}

export const OctokitWithThrottling = Octokit.plugin(throttling);

/**
 * Time-to-live for cached Octokit instances (5 minutes).
 * Short TTL ensures refreshed tokens are picked up promptly after expiry/refresh.
 * GitHub App tokens last ~8 hours, so 5 min cache provides good balance
 * between performance and token freshness.
 */
const TOKEN_TTL_MS = 5 * 60 * 1000; // 5 minutes

/**
 * Maximum number of cached Octokit instances.
 * Each instance holds a Bottleneck instance (throttling plugin) which
 * consumes non-trivial memory. Cap prevents unbounded growth when many
 * different OAuth tokens are used (e.g. multi-user MCP server).
 */
const MAX_INSTANCES = 50;

/**
 * Interval for periodic cleanup of expired instances (60 seconds).
 * This prevents expired instances from lingering in memory indefinitely.
 */
const PURGE_INTERVAL_MS = 60 * 1000;

/**
 * Cached Octokit instance with creation timestamp for TTL checks.
 */
interface CachedInstance {
  client: InstanceType<typeof OctokitWithThrottling>;
  createdAt: number;
}

/**
 * Check if a cached instance has expired based on TTL.
 */
function isExpired(cached: CachedInstance): boolean {
  return Date.now() - cached.createdAt > TOKEN_TTL_MS;
}

// Cache instances by token hash (or 'DEFAULT' for the default token)
const instances = new Map<string, CachedInstance>();
// Track pending default creation to handle race conditions
let pendingDefaultPromise: Promise<
  InstanceType<typeof OctokitWithThrottling>
> | null = null;

// Periodic purge timer ref (unref'd so it doesn't block process exit)
let purgeTimer: ReturnType<typeof setInterval> | null = null;

/**
 * Evict all expired instances from the cache.
 * Also enforces MAX_INSTANCES by removing the oldest entries when over capacity.
 */
function purgeExpiredInstances(): void {
  // 1. Remove expired entries
  for (const [key, cached] of instances.entries()) {
    if (isExpired(cached)) {
      instances.delete(key);
    }
  }

  // 2. If still over capacity, evict oldest non-DEFAULT entries
  if (instances.size > MAX_INSTANCES) {
    const sorted = [...instances.entries()]
      .filter(([key]) => key !== 'DEFAULT')
      .sort((a, b) => a[1].createdAt - b[1].createdAt);

    const excess = instances.size - MAX_INSTANCES;
    for (let i = 0; i < excess && i < sorted.length; i++) {
      const entry = sorted[i];
      if (entry) instances.delete(entry[0]);
    }
  }
}

/**
 * Start the periodic purge timer if not already running.
 * Timer is unref'd so it does not prevent Node.js process exit.
 */
function ensurePurgeTimer(): void {
  if (purgeTimer) return;
  purgeTimer = setInterval(purgeExpiredInstances, PURGE_INTERVAL_MS);
  // Unref so this timer doesn't keep the process alive
  if (typeof purgeTimer === 'object' && 'unref' in purgeTimer) {
    purgeTimer.unref();
  }
}

/**
 * Maximum number of retries for rate-limited requests.
 */
const MAX_RATE_LIMIT_RETRIES = 3;

/**
 * Maximum wait time (in seconds) before refusing to retry.
 * Prevents waiting too long for rate limit reset.
 */
const MAX_RETRY_AFTER_SECONDS = 60;

function recordThrottleRateLimit(
  limitType: 'primary' | 'secondary',
  retryAfter: number,
  options: { method: string; url: string }
): void {
  void logRateLimit({
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

    // Retry if under max retries and wait is reasonable
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

  const options: OctokitOptions & {
    throttle: ReturnType<typeof createThrottleOptions>;
  } = {
    userAgent: `octocode-mcp/${version}`,
    baseUrl,
    request: { timeout: config.timeout || 30000 },
    throttle: createThrottleOptions(),
    ...(token && { auth: token }),
  };

  return new OctokitWithThrottling(options);
}

export async function getOctokit(
  authInfo?: AuthInfo
): Promise<InstanceType<typeof OctokitWithThrottling>> {
  // Start periodic cleanup on first call
  ensurePurgeTimer();

  // Case 1: Specific Auth Info provided
  if (authInfo?.token) {
    // Use hashed token as key to avoid storing raw tokens in memory
    const key = hashToken(authInfo.token);
    const cached = instances.get(key);

    if (cached && !isExpired(cached)) {
      return cached.client;
    }

    // Purge expired entries before adding a new one to stay within limits
    if (instances.size >= MAX_INSTANCES) {
      purgeExpiredInstances();
    }

    const newInstance = createOctokitInstance(authInfo.token);
    instances.set(key, { client: newInstance, createdAt: Date.now() });
    return newInstance;
  }

  // Case 2: Default instance already exists and not expired
  const defaultCached = instances.get('DEFAULT');
  if (defaultCached && !isExpired(defaultCached)) {
    return defaultCached.client;
  }

  // Case 3: Default instance being created (race condition protection)
  if (pendingDefaultPromise) {
    return pendingDefaultPromise;
  }

  // Case 4: Create new default instance
  pendingDefaultPromise = (async () => {
    try {
      const token = await getGitHubToken();
      const instance = createOctokitInstance(token ?? undefined);
      instances.set('DEFAULT', { client: instance, createdAt: Date.now() });
      return instance;
    } finally {
      pendingDefaultPromise = null;
    }
  })();

  return pendingDefaultPromise;
}

/**
 * Maximum number of entries in the default branch cache.
 * FIFO eviction when full — Map preserves insertion order.
 */
export const MAX_BRANCH_CACHE_SIZE = 200;

/**
 * In-memory cache for default branch lookups.
 * Key: "owner/repo", Value: branch name.
 * TTL is tied to the Octokit instance cache (5 min) — cleared via clearOctokitInstances().
 *
 * Prevents redundant GitHub API calls when multiple queries in a bulk
 * operation target the same repo without specifying a branch.
 */
const defaultBranchCache = new Map<string, string>();

/**
 * Fetch the default branch name for a GitHub repository.
 * Results are cached in-memory to avoid redundant API calls within a session.
 *
 * Resolution strategy:
 *   1. repos.get API → definitive default_branch from GitHub
 *   2. Smart fallback: verify 'main' branch exists → verify 'master' exists
 *   3. Throw if all attempts fail (repo may not exist or be inaccessible)
 *
 * Shared utility — all GitHub tools that accept an optional `branch` must
 * use this function when the caller omits the branch parameter.
 */
export async function resolveDefaultBranch(
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<string> {
  const cacheKey = `${owner}/${repo}`;
  const cached = defaultBranchCache.get(cacheKey);
  if (cached) return cached;

  const octokit = await getOctokit(authInfo);

  // Primary: repos.get returns the canonical default branch
  try {
    const { data } = await octokit.rest.repos.get({ owner, repo });
    const branch = data.default_branch;
    cacheDefaultBranch(cacheKey, branch);
    return branch;
  } catch {
    // repos.get failed (auth/network/repo); try probing well-known branch names below.
  }

  // Smart fallback: verify common branch names exist (main → master)
  const candidates = ['main', 'master'] as const;
  for (const candidate of candidates) {
    try {
      await octokit.rest.repos.getBranch({ owner, repo, branch: candidate });
      cacheDefaultBranch(cacheKey, candidate);
      return candidate;
    } catch {
      // Branch not found — try next candidate
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

/**
 * Clear all cached Octokit instances and stop the purge timer.
 * Used for testing, shutdown, or when a full reset is needed.
 */
export function clearOctokitInstances(): void {
  instances.clear();
  pendingDefaultPromise = null;
  defaultBranchCache.clear();

  if (purgeTimer) {
    clearInterval(purgeTimer);
    purgeTimer = null;
  }
}
