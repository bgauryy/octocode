import type {
  ICodeHostProvider,
  ProviderType,
  ProviderConfig,
} from './types.js';
import { GitHubProvider } from './github/GitHubProvider.js';
import { createHash } from 'crypto';

const PROVIDER_CACHE_TTL_MS = 60 * 60 * 1000;

const MAX_PROVIDER_INSTANCES = 20;

interface CachedProvider {
  provider: ICodeHostProvider;
  createdAt: number;
  lastAccessedAt: number;
}

const instanceCache = new Map<string, CachedProvider>();

function isProviderCacheValid(entry: CachedProvider): boolean {
  return Date.now() - entry.createdAt < PROVIDER_CACHE_TTL_MS;
}

function evictProviderInstances(): void {
  for (const [key, entry] of instanceCache.entries()) {
    if (!isProviderCacheValid(entry)) {
      instanceCache.delete(key);
    }
  }

  if (instanceCache.size > MAX_PROVIDER_INSTANCES) {
    const sorted = [...instanceCache.entries()].sort(
      (a, b) => a[1].lastAccessedAt - b[1].lastAccessedAt
    );
    const excess = instanceCache.size - MAX_PROVIDER_INSTANCES;
    for (let i = 0; i < excess && i < sorted.length; i++) {
      const entry = sorted[i];
      if (entry) instanceCache.delete(entry[0]);
    }
  }
}

function hashToken(token?: string): string {
  if (!token) return 'default';
  return createHash('sha256').update(token).digest('hex').slice(0, 16);
}

function normalizeUrl(url: string): string {
  if (url === 'default') return url;

  try {
    const parsed = new URL(url);
    let normalized = `${parsed.protocol}//${parsed.hostname.toLowerCase()}`;
    if (parsed.port) normalized += `:${parsed.port}`;
    normalized += parsed.pathname.replace(/\/+$/, '') || '';
    return normalized;
  } catch {
    return url.replace(/\/+$/, '');
  }
}

function getCacheKey(type: ProviderType, config?: ProviderConfig): string {
  const baseUrl = normalizeUrl(config?.baseUrl || 'default');
  const tokenHash = hashToken(config?.token || config?.authInfo?.token);
  return `${type}:${baseUrl}:${tokenHash}`;
}

export function getProvider(
  type: ProviderType = 'github',
  config?: ProviderConfig
): ICodeHostProvider {
  if (type !== 'github') {
    throw new Error(
      `Unknown provider type: '${type}'. Only 'github' is supported.`
    );
  }

  const cacheKey = getCacheKey(type, config);

  const cached = instanceCache.get(cacheKey);
  if (cached && isProviderCacheValid(cached)) {
    cached.lastAccessedAt = Date.now();
    return cached.provider;
  }

  if (cached) {
    instanceCache.delete(cacheKey);
  }

  if (instanceCache.size >= MAX_PROVIDER_INSTANCES) {
    evictProviderInstances();
  }

  const provider = new GitHubProvider({ ...config, type });

  const now = Date.now();
  instanceCache.set(cacheKey, {
    provider,
    createdAt: now,
    lastAccessedAt: now,
  });
  return provider;
}

export function clearProviderCache(): void {
  instanceCache.clear();
}

export interface ProviderDiagnostic {
  provider: string;
  ok: boolean;
  error?: string;
}

export async function initializeProviders(): Promise<ProviderDiagnostic[]> {
  try {
    getProvider('github');
    return [{ provider: 'github', ok: true }];
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    process.stderr.write(
      `⚠️  github provider failed to initialize: ${message}\n`
    );
    return [{ provider: 'github', ok: false, error: message }];
  }
}
