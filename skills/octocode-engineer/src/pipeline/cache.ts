import fs from 'node:fs';
import path from 'node:path';

export const ANALYSIS_SCHEMA_VERSION = '1.1.0';

interface CacheEntry {
  mtimeMs: number;
  sizeBytes: number;
  result: unknown;
  lastAccessMs: number;
}

interface AnalysisCache {
  version: number;
  schemaVersion: string;
  root: string;
  entries: Record<string, CacheEntry>;
}

const CACHE_VERSION = 1;
const DEFAULT_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

export function loadCache(root: string): AnalysisCache | null {
  const cachePath = path.join(
    root,
    '.octocode',
    'scan',
    '.cache',
    'analysis-cache.json'
  );
  try {
    const data = JSON.parse(fs.readFileSync(cachePath, 'utf8'));
    if (
      data.version !== CACHE_VERSION ||
      data.root !== root ||
      data.schemaVersion !== ANALYSIS_SCHEMA_VERSION
    )
      return null;
    return data;
  } catch {
    return null;
  }
}

export function saveCache(root: string, cache: AnalysisCache): void {
  const dir = path.join(root, '.octocode', 'scan', '.cache');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'analysis-cache.json'),
    JSON.stringify(cache),
    'utf8'
  );
}

export function clearCache(root: string): void {
  const cachePath = path.join(
    root,
    '.octocode',
    'scan',
    '.cache',
    'analysis-cache.json'
  );
  try {
    fs.unlinkSync(cachePath);
  } catch {
    void 0;
  }
}

export function isCacheHit(
  cache: AnalysisCache | null,
  relPath: string,
  stat: { mtimeMs: number; size: number }
): boolean {
  if (!cache) return false;
  const entry = cache.entries[relPath];
  if (!entry) return false;
  return entry.mtimeMs === stat.mtimeMs && entry.sizeBytes === stat.size;
}

export function getCachedResult(
  cache: AnalysisCache,
  relPath: string
): unknown {
  const entry = cache.entries[relPath];
  if (entry) {
    entry.lastAccessMs = Date.now();
  }
  return entry?.result;
}

export function setCacheEntry(
  cache: AnalysisCache,
  relPath: string,
  stat: { mtimeMs: number; size: number },
  result: unknown
): void {
  cache.entries[relPath] = {
    mtimeMs: stat.mtimeMs,
    sizeBytes: stat.size,
    result,
    lastAccessMs: Date.now(),
  };
}

export function createEmptyCache(root: string): AnalysisCache {
  return {
    version: CACHE_VERSION,
    schemaVersion: ANALYSIS_SCHEMA_VERSION,
    root,
    entries: {},
  };
}

export function garbageCollect(
  cache: AnalysisCache,
  maxAgeMs: number = DEFAULT_MAX_AGE_MS
): number {
  const now = Date.now();
  const keysToRemove: string[] = [];
  for (const [key, entry] of Object.entries(cache.entries)) {
    if (now - entry.lastAccessMs > maxAgeMs) {
      keysToRemove.push(key);
    }
  }
  for (const key of keysToRemove) {
    delete cache.entries[key];
  }
  return keysToRemove.length;
}
