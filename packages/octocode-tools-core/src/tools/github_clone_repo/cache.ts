import {
  existsSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
  rmSync,
  readdirSync,
  statSync,
  rmdirSync,
} from 'node:fs';
import { join } from 'node:path';
import type { CloneCacheMeta, CacheSource } from './types.js';
import { getDirectorySizeBytes } from '../../shared/fs-utils.js';
import {
  getCloneBaseDir,
  getTreeBaseDir,
  getTreeLockDir,
  getCloneLockDir,
} from './cachePaths.js';
import { tryAcquireMaterializationLock } from './cacheArtifacts.js';

const DEFAULT_CACHE_TTL_MS = 24 * 60 * 60 * 1000;

const DEFAULT_MAX_CACHE_SIZE_BYTES = 2 * 1024 * 1024 * 1024;

const DEFAULT_MAX_CLONE_COUNT = 50;

const META_FILE_NAME = '.octocode-clone-meta.json';

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null;
}

function parseCacheMeta(raw: unknown): CloneCacheMeta | null {
  if (!isRecord(raw)) return null;
  if (typeof raw.clonedAt !== 'string') return null;
  if (typeof raw.expiresAt !== 'string') return null;
  if (typeof raw.owner !== 'string') return null;
  if (typeof raw.repo !== 'string') return null;
  if (typeof raw.branch !== 'string') return null;
  if (raw.source !== 'clone' && raw.source !== 'treeFetch') {
    return null;
  }
  const meta: CloneCacheMeta = {
    clonedAt: raw.clonedAt,
    expiresAt: raw.expiresAt,
    owner: raw.owner,
    repo: raw.repo,
    branch: raw.branch,
    source: raw.source,
  };
  if (typeof raw.sparsePath === 'string') meta.sparsePath = raw.sparsePath;
  if (typeof raw.sizeBytes === 'number') meta.sizeBytes = raw.sizeBytes;
  if (
    typeof raw.snapshotId === 'string' &&
    /^[0-9a-f-]{36}$/.test(raw.snapshotId)
  )
    meta.snapshotId = raw.snapshotId;
  if (typeof raw.commitSha === 'string' && raw.commitSha.length === 40)
    meta.commitSha = raw.commitSha;
  return meta;
}

export function readCacheMeta(cloneDir: string): CloneCacheMeta | null {
  const metaPath = join(cloneDir, META_FILE_NAME);
  if (!existsSync(metaPath)) return null;
  try {
    return parseCacheMeta(JSON.parse(readFileSync(metaPath, 'utf-8')));
  } catch {
    return null;
  }
}

export function writeCacheMeta(cloneDir: string, meta: CloneCacheMeta): void {
  try {
    writeFileSync(
      join(cloneDir, META_FILE_NAME),
      JSON.stringify(meta, null, 2),
      'utf-8'
    );
  } catch {
    void 0;
  }
}

export function isCacheValid(meta: CloneCacheMeta): boolean {
  return Date.now() < new Date(meta.expiresAt).getTime();
}

export function isCacheHit(
  cloneDir: string
): { hit: true; meta: CloneCacheMeta } | { hit: false } {
  const meta = readCacheMeta(cloneDir);
  if (!meta) return { hit: false };
  if (!isCacheValid(meta)) return { hit: false };
  if (!existsSync(cloneDir)) return { hit: false };
  return { hit: true, meta };
}

export function getCacheTTL(): number {
  const raw = process.env.OCTOCODE_CACHE_TTL_MS;
  if (raw != null) {
    const parsed = Number(raw);
    if (!Number.isNaN(parsed) && parsed > 0) return parsed;
  }
  return DEFAULT_CACHE_TTL_MS;
}

export function getMaxCacheSizeBytes(): number {
  const raw = process.env.OCTOCODE_MAX_CACHE_SIZE;
  if (raw != null) {
    const parsed = Number(raw);
    if (!Number.isNaN(parsed) && parsed > 0) return parsed;
  }
  return DEFAULT_MAX_CACHE_SIZE_BYTES;
}

export function getMaxCloneCount(): number {
  const raw = process.env.OCTOCODE_MAX_CLONES;
  if (raw != null) {
    const parsed = Number(raw);
    if (!Number.isNaN(parsed) && parsed > 0) return Math.floor(parsed);
  }
  return DEFAULT_MAX_CLONE_COUNT;
}

export function createCacheMeta(
  owner: string,
  repo: string,
  branch: string,
  source: CacheSource,
  sparsePath?: string,
  sizeBytes?: number,
  commitSha?: string
): CloneCacheMeta {
  const now = new Date();
  return {
    clonedAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + getCacheTTL()).toISOString(),
    owner,
    repo,
    branch,
    source,
    ...(sparsePath ? { sparsePath } : {}),
    ...(sizeBytes != null ? { sizeBytes } : {}),
    ...(commitSha ? { commitSha } : {}),
  };
}

export function ensureCloneParentDir(cloneDir: string): void {
  const parent = join(cloneDir, '..');
  try {
    if (!existsSync(parent)) {
      mkdirSync(parent, { recursive: true, mode: 0o700 });
    }
  } catch (error) {
    throw new Error(
      `Failed to create clone parent directory '${parent}': ${error instanceof Error ? error.message : String(error)}`,
      { cause: error }
    );
  }
}

export function removeCloneDir(cloneDir: string): void {
  try {
    if (existsSync(cloneDir)) {
      rmSync(cloneDir, { recursive: true, force: true });
    }
  } catch {
    void 0;
  }
}

function isDir(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function listDir(path: string): string[] {
  try {
    return readdirSync(path);
  } catch {
    return [];
  }
}

function* walkCloneDirs(reposBase: string): Generator<string> {
  for (const ownerName of listDir(reposBase)) {
    const ownerDir = join(reposBase, ownerName);
    if (!isDir(ownerDir)) continue;
    for (const repoName of listDir(ownerDir)) {
      const repoDir = join(ownerDir, repoName);
      if (!isDir(repoDir)) continue;
      for (const branchName of listDir(repoDir)) {
        const branchDir = join(repoDir, branchName);
        if (isDir(branchDir)) yield branchDir;
      }
    }
  }
}

function cleanupEmptyDirectories(reposBase: string): void {
  for (const ownerName of [...listDir(reposBase)]) {
    const ownerDir = join(reposBase, ownerName);
    if (!isDir(ownerDir)) continue;

    for (const repoName of [...listDir(ownerDir)]) {
      const repoDir = join(ownerDir, repoName);
      if (!isDir(repoDir)) continue;
      if (listDir(repoDir).length === 0) {
        try {
          rmdirSync(repoDir);
        } catch {
          /* best-effort */
        }
      }
    }

    if (listDir(ownerDir).length === 0) {
      try {
        rmdirSync(ownerDir);
      } catch {
        /* best-effort */
      }
    }
  }
}

type AcquireEntryLock = (path: string) => (() => void) | undefined;

function evictExpiredEntries(
  reposBase: string,
  acquireEntry: AcquireEntryLock
): number {
  let evicted = 0;
  for (const branchDir of walkCloneDirs(reposBase)) {
    const release = acquireEntry(branchDir);
    if (!release) continue;
    try {
      const meta = readCacheMeta(branchDir);
      if (!meta || !isCacheValid(meta)) {
        rmSync(branchDir, { recursive: true, force: true });
        evicted++;
      }
    } catch {
      /* skip single entry */
    } finally {
      release();
    }
  }
  return evicted;
}

interface LiveCacheEntry {
  branchDir: string;
  clonedAtMs: number;
  sizeBytes: number;
}

function collectLiveEntries(
  reposBase: string,
  acquireEntry: AcquireEntryLock
): LiveCacheEntry[] {
  const entries: LiveCacheEntry[] = [];
  for (const branchDir of walkCloneDirs(reposBase)) {
    const release = acquireEntry(branchDir);
    if (!release) continue;
    try {
      const meta = readCacheMeta(branchDir);
      if (!meta) continue;
      const clonedAtMs = Number.isNaN(Date.parse(meta.clonedAt))
        ? 0
        : Date.parse(meta.clonedAt);
      entries.push({
        branchDir,
        clonedAtMs,
        sizeBytes: meta.sizeBytes ?? getDirectorySizeBytes(branchDir),
      });
    } finally {
      release();
    }
  }
  return entries;
}

function evictByCapacity(
  entries: LiveCacheEntry[],
  maxSizeBytes: number,
  maxCloneCount: number,
  acquireEntry: AcquireEntryLock
): number {
  let totalSize = entries.reduce((sum, e) => sum + e.sizeBytes, 0);
  let totalCount = entries.length;
  if (totalSize <= maxSizeBytes && totalCount <= maxCloneCount) return 0;

  entries.sort((a, b) => a.clonedAtMs - b.clonedAtMs);
  let evicted = 0;
  for (const entry of entries) {
    if (totalSize <= maxSizeBytes && totalCount <= maxCloneCount) break;
    const release = acquireEntry(entry.branchDir);
    if (!release) continue;
    try {
      rmSync(entry.branchDir, { recursive: true, force: true });
      evicted++;
      totalSize -= entry.sizeBytes;
      totalCount -= 1;
    } catch {
      /* best-effort */
    } finally {
      release();
    }
  }
  return evicted;
}

function evictExpiredCacheBase(
  cacheBase: string,
  acquireEntry: AcquireEntryLock
): number {
  if (!existsSync(cacheBase)) return 0;

  let evicted = 0;
  try {
    evicted += evictExpiredEntries(cacheBase, acquireEntry);
  } catch {
    return evicted;
  }

  cleanupEmptyDirectories(cacheBase);

  const lruEvicted = evictByCapacity(
    collectLiveEntries(cacheBase, acquireEntry),
    getMaxCacheSizeBytes(),
    getMaxCloneCount(),
    acquireEntry
  );
  evicted += lruEvicted;

  if (lruEvicted > 0) cleanupEmptyDirectories(cacheBase);

  return evicted;
}

export function evictExpiredClones(octocodeDir: string): number {
  return evictExpiredCacheBase(getCloneBaseDir(octocodeDir), path =>
    tryAcquireMaterializationLock(getCloneLockDir(octocodeDir, path))
  );
}

export function evictExpiredTrees(octocodeDir: string): number {
  return evictExpiredCacheBase(getTreeBaseDir(octocodeDir), path =>
    tryAcquireMaterializationLock(getTreeLockDir(octocodeDir, path))
  );
}
