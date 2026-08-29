import { createHash } from 'node:crypto';
import {
  chmod,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { join } from 'node:path';
import { getDefaultOctocodeHome } from '../../../shared/paths.js';

const DISK_CACHE_VERSION = 1;
const DEFAULT_MAX_ENTRY_BYTES = 5 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES = 5000;
const DEFAULT_MAX_BYTES = 256 * 1024 * 1024;
const DEFAULT_SWEEP_INTERVAL_MS = 24 * 60 * 60 * 1000;
const TEMP_MAX_AGE_MS = 15 * 60 * 1000;
const SWEEP_MARKER = '.last-gc';

interface DiskCacheEntry<T> {
  version: typeof DISK_CACHE_VERSION;
  key: string;
  createdAt: number;
  expiresAt: number;
  staleUntil: number;
  value: T;
  etag?: string;
}

export interface DiskCacheRead<T> {
  value: T;
  etag?: string;
  state: 'fresh' | 'stale';
  expiresAt: number;
}

const diskStats = {
  hits: 0,
  misses: 0,
  sets: 0,
  errors: 0,
  evictions: 0,
};

function positiveEnv(name: string, fallback: number): number {
  const parsed = Number(process.env[name]);
  return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : fallback;
}

export function isDiskCacheEnabled(): boolean {
  const raw = process.env.OCTOCODE_DISK_CACHE?.trim().toLowerCase();
  if (raw === 'false' || raw === '0') return false;
  if (process.env.NODE_ENV === 'test') return raw === 'true' || raw === '1';
  return true;
}

export function getDiskCacheRoot(
  octocodeHome = getDefaultOctocodeHome()
): string {
  return join(octocodeHome, 'tmp', 'response');
}

function cachePrefix(key: string): string {
  const raw = key.match(/^v\d+-([^:]+):/)?.[1] ?? 'default';
  return raw.replace(/[^a-zA-Z0-9._-]/g, '_');
}

export function getDiskCacheEntryPath(
  key: string,
  octocodeHome = getDefaultOctocodeHome()
): string {
  const hash = createHash('sha256').update(key).digest('hex');
  return join(
    getDiskCacheRoot(octocodeHome),
    cachePrefix(key),
    hash.slice(0, 2),
    `${hash}.json`
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseEntry<T>(
  raw: string,
  expectedKey: string
): DiskCacheEntry<T> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed)) return null;
  if (parsed.version !== DISK_CACHE_VERSION || parsed.key !== expectedKey)
    return null;
  if (
    typeof parsed.createdAt !== 'number' ||
    typeof parsed.expiresAt !== 'number'
  )
    return null;
  if (typeof parsed.staleUntil !== 'number' || !('value' in parsed))
    return null;
  if (parsed.etag !== undefined && typeof parsed.etag !== 'string') return null;
  return parsed as unknown as DiskCacheEntry<T>;
}

async function removeBestEffort(path: string): Promise<void> {
  try {
    await rm(path, { force: true, recursive: true });
  } catch {
    void 0;
  }
}

export async function readDiskCache<T>(
  key: string,
  options: { allowStale?: boolean } = {}
): Promise<DiskCacheRead<T> | undefined> {
  if (!isDiskCacheEnabled()) return undefined;
  const path = getDiskCacheEntryPath(key);
  try {
    const info = await stat(path);
    if (
      !info.isFile() ||
      info.size >
        positiveEnv(
          'OCTOCODE_DISK_CACHE_MAX_ENTRY_SIZE',
          DEFAULT_MAX_ENTRY_BYTES
        )
    ) {
      await removeBestEffort(path);
      diskStats.errors++;
      return undefined;
    }
    const entry = parseEntry<T>(await readFile(path, 'utf8'), key);
    if (!entry) {
      await removeBestEffort(path);
      diskStats.errors++;
      return undefined;
    }
    const now = Date.now();
    if (now >= entry.staleUntil) {
      await removeBestEffort(path);
      diskStats.evictions++;
      diskStats.misses++;
      return undefined;
    }
    if (now >= entry.expiresAt && !options.allowStale) {
      diskStats.misses++;
      return undefined;
    }
    diskStats.hits++;
    return {
      value: entry.value,
      ...(entry.etag ? { etag: entry.etag } : {}),
      state: now < entry.expiresAt ? 'fresh' : 'stale',
      expiresAt: entry.expiresAt,
    };
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') diskStats.errors++;
    diskStats.misses++;
    return undefined;
  }
}

export async function writeDiskCache<T>(
  key: string,
  value: T,
  ttlSeconds: number,
  options: { etag?: string; staleTtlSeconds?: number } = {}
): Promise<boolean> {
  if (!isDiskCacheEnabled()) return false;
  const now = Date.now();
  const ttlMs = Math.max(1, ttlSeconds * 1000);
  const staleTtlMs = Math.max(
    ttlMs,
    (options.staleTtlSeconds ?? ttlSeconds) * 1000
  );
  const entry: DiskCacheEntry<T> = {
    version: DISK_CACHE_VERSION,
    key,
    createdAt: now,
    expiresAt: now + ttlMs,
    staleUntil: now + staleTtlMs,
    value,
    ...(options.etag ? { etag: options.etag } : {}),
  };
  let serialized: string | undefined;
  try {
    serialized = JSON.stringify(entry);
  } catch {
    diskStats.errors++;
    return false;
  }
  if (
    serialized === undefined ||
    Buffer.byteLength(serialized) >
      positiveEnv('OCTOCODE_DISK_CACHE_MAX_ENTRY_SIZE', DEFAULT_MAX_ENTRY_BYTES)
  ) {
    return false;
  }

  const path = getDiskCacheEntryPath(key);
  const parent = join(path, '..');
  const tempPath = `${path}.tmp-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  try {
    await mkdir(parent, { recursive: true, mode: 0o700 });
    await chmod(parent, 0o700);
    await writeFile(tempPath, serialized, { encoding: 'utf8', mode: 0o600 });
    await chmod(tempPath, 0o600);
    try {
      await rename(tempPath, path);
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'EEXIST' && code !== 'EPERM') throw error;
      await removeBestEffort(path);
      await rename(tempPath, path);
    }
    diskStats.sets++;
    return true;
  } catch {
    diskStats.errors++;
    await removeBestEffort(tempPath);
    return false;
  }
}

async function collectResponseFiles(root: string): Promise<string[]> {
  const files: string[] = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop()!;
    let entries;
    try {
      entries = await readdir(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) stack.push(path);
      else if (entry.isFile() && entry.name !== SWEEP_MARKER) files.push(path);
    }
  }
  return files;
}

async function sweepDiskCache(
  octocodeHome: string,
  now: number,
  onlyIfDue: boolean
): Promise<number> {
  if (!isDiskCacheEnabled()) return 0;
  const root = getDiskCacheRoot(octocodeHome);
  const marker = join(root, SWEEP_MARKER);
  if (onlyIfDue) {
    try {
      const markerStat = await stat(marker);
      if (now - markerStat.mtimeMs < DEFAULT_SWEEP_INTERVAL_MS) return 0;
    } catch {
      void 0;
    }
  }

  let evicted = 0;
  const files = await collectResponseFiles(root);
  const maxEntries = positiveEnv(
    'OCTOCODE_DISK_CACHE_MAX_ENTRIES',
    DEFAULT_MAX_ENTRIES
  );
  const maxBytes = positiveEnv(
    'OCTOCODE_DISK_CACHE_MAX_SIZE',
    DEFAULT_MAX_BYTES
  );
  const live: Array<{ path: string; size: number; modified: number }> = [];
  let totalBytes = 0;

  for (const path of files) {
    try {
      const info = await stat(path);
      if (path.includes('.tmp-')) {
        if (now - info.mtimeMs > TEMP_MAX_AGE_MS) {
          await removeBestEffort(path);
          evicted++;
        }
        continue;
      }
      const raw = await readFile(path, 'utf8');
      let parsed: unknown;
      try {
        parsed = JSON.parse(raw);
      } catch {
        parsed = null;
      }
      if (
        !isRecord(parsed) ||
        typeof parsed.staleUntil !== 'number' ||
        now >= parsed.staleUntil
      ) {
        await removeBestEffort(path);
        evicted++;
        continue;
      }
      live.push({ path, size: info.size, modified: info.mtimeMs });
      totalBytes += info.size;
    } catch {
      void 0;
    }
  }

  live.sort((a, b) => a.modified - b.modified);
  while (live.length > maxEntries || totalBytes > maxBytes) {
    const oldest = live.shift();
    if (!oldest) break;
    await removeBestEffort(oldest.path);
    totalBytes -= oldest.size;
    evicted++;
  }

  try {
    await mkdir(root, { recursive: true, mode: 0o700 });
    await writeFile(marker, String(now), { encoding: 'utf8', mode: 0o600 });
  } catch {
    diskStats.errors++;
  }
  diskStats.evictions += evicted;
  return evicted;
}

export function sweepDiskCacheIfDue(
  octocodeHome = getDefaultOctocodeHome(),
  now = Date.now()
): Promise<number> {
  return sweepDiskCache(octocodeHome, now, true);
}

/** Full response-cache sweep for the already due-gated shared lifecycle. */
export function sweepDiskCacheNow(
  octocodeHome = getDefaultOctocodeHome(),
  now = Date.now()
): Promise<number> {
  return sweepDiskCache(octocodeHome, now, false);
}

export async function clearDiskCache(
  octocodeHome = getDefaultOctocodeHome()
): Promise<void> {
  await removeBestEffort(getDiskCacheRoot(octocodeHome));
}

export function getDiskCacheStats(): Readonly<typeof diskStats> {
  return { ...diskStats };
}

export function resetDiskCacheStats(): void {
  diskStats.hits = 0;
  diskStats.misses = 0;
  diskStats.sets = 0;
  diskStats.errors = 0;
  diskStats.evictions = 0;
}
