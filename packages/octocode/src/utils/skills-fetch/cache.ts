import type {
  MarketplaceSource,
  MarketplaceSkill,
} from '../../configs/skills-marketplace.js';
import {
  dirExists,
  writeFileContent,
  readFileContent,
  fileExists,
} from '../fs.js';
import { join } from 'node:path';
import { mkdirSync, statSync, readdirSync, unlinkSync } from 'node:fs';
import os from 'node:os';
import { trySafe } from '../try-safe.js';
import { CACHE_TTL_MS } from './constants.js';
import { CachedSkillsDataSchema, type CachedSkillsData } from './types.js';

export function getCacheDir(): string {
  const home = os.homedir();

  const npmCacheDir = process.env.npm_config_cache || join(home, '.npm');
  const npmOctocodeCache = join(npmCacheDir, '_cacache', 'octocode-skills');

  const isWindows = os.platform() === 'win32';
  const fallbackCache = isWindows
    ? join(
        process.env.LOCALAPPDATA || join(home, 'AppData', 'Local'),
        'octocode',
        'cache',
        'skills'
      )
    : join(home, '.cache', 'octocode', 'skills');

  if (dirExists(npmCacheDir)) {
    return npmOctocodeCache;
  }

  return fallbackCache;
}

function ensureCacheDir(): string {
  const cacheDir = getCacheDir();
  if (!dirExists(cacheDir)) {
    mkdirSync(cacheDir, { recursive: true, mode: 0o700 });
  }
  return cacheDir;
}

function getCacheFilePath(source: MarketplaceSource): string {
  const cacheDir = ensureCacheDir();
  return join(cacheDir, `${source.id}.json`);
}

function isCacheValid(cacheFile: string): boolean {
  return trySafe(() => {
    if (!fileExists(cacheFile)) {
      return false;
    }
    const stats = statSync(cacheFile);
    const age = Date.now() - stats.mtimeMs;
    return age < CACHE_TTL_MS;
  }, false);
}

export function readCachedSkills(
  source: MarketplaceSource
): MarketplaceSkill[] | null {
  return trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    if (!isCacheValid(cacheFile)) {
      return null;
    }
    const content = readFileContent(cacheFile);
    if (!content) {
      return null;
    }
    const raw = JSON.parse(content);
    const validated = CachedSkillsDataSchema.safeParse(raw);
    if (!validated.success) {
      return null;
    }
    const data = raw as CachedSkillsData;
    if (Date.now() - data.timestamp > CACHE_TTL_MS) {
      return null;
    }
    return data.skills.map(skill => ({
      ...skill,
      source,
    }));
  }, null);
}

export function writeCachedSkills(
  source: MarketplaceSource,
  skills: MarketplaceSkill[]
): void {
  trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    const skillsToCache = skills.map(({ source: _, ...rest }) => rest);
    const data: CachedSkillsData = {
      timestamp: Date.now(),
      skills: skillsToCache as MarketplaceSkill[],
    };
    writeFileContent(cacheFile, JSON.stringify(data, null, 2));
    return true;
  }, false);
}

export function clearSkillsCache(): void {
  trySafe(() => {
    const cacheDir = getCacheDir();
    if (dirExists(cacheDir)) {
      const files = readdirSync(cacheDir);
      for (const file of files) {
        if (file.endsWith('.json')) {
          unlinkSync(join(cacheDir, file));
        }
      }
    }
    return true;
  }, false);
}

export function clearSourceCache(source: MarketplaceSource): void {
  trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    if (fileExists(cacheFile)) {
      unlinkSync(cacheFile);
    }
    return true;
  }, false);
}

export function getCacheInfo(source: MarketplaceSource): {
  isCached: boolean;
  age: number | null;
  expiresIn: number | null;
} {
  return trySafe(
    () => {
      const cacheFile = getCacheFilePath(source);
      if (!fileExists(cacheFile)) {
        return { isCached: false, age: null, expiresIn: null };
      }
      const stats = statSync(cacheFile);
      const age = Date.now() - stats.mtimeMs;
      const expiresIn = Math.max(0, CACHE_TTL_MS - age);
      return {
        isCached: age < CACHE_TTL_MS,
        age,
        expiresIn: age < CACHE_TTL_MS ? expiresIn : null,
      };
    },
    { isCached: false, age: null, expiresIn: null }
  );
}

export function getSkillsCacheDir(): string {
  return getCacheDir();
}
