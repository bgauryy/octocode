import { fetchWithRetries } from '../../http/fetch.js';
import { countSerializedChars } from '../../response/charSavings.js';
import type { NpmPackageResult } from '../types.js';
import type { NpmCliSearchItem, NpmViewResult } from './npmRegistry.js';

export function cleanRepoUrl(url: string): string {
  return url.replace(/^git\+/, '').replace(/\.git$/, '');
}

const NPM_DOWNLOADS_API = 'https://api.npmjs.org/downloads/point/last-week';

export function countRawPayloadChars(raw: unknown): number {
  return raw === undefined ? 0 : countSerializedChars(raw);
}

export async function fetchWeeklyDownloads(
  packageName: string
): Promise<{ downloads?: number; rawResponseChars: number }> {
  try {
    const url = `${NPM_DOWNLOADS_API}/${encodeURIComponent(packageName)}`;
    const data = (await fetchWithRetries(url, {
      maxRetries: 0,
      initialDelayMs: 300,
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(8000),
      packageRegistry: 'npm',
    })) as { downloads?: number } | null;
    return {
      downloads:
        typeof data?.downloads === 'number' ? data.downloads : undefined,
      rawResponseChars: countRawPayloadChars(data),
    };
  } catch {
    return { rawResponseChars: 0 };
  }
}

function mapExports(value: unknown): string[] | undefined {
  if (typeof value === 'string') return [value];
  if (!value || typeof value !== 'object') return undefined;
  const record = value as Record<string, unknown>;
  const entries = Object.entries(record)
    .flatMap(([key, entry]) => {
      if (typeof entry === 'string') return [`${key}:${entry}`];
      if (entry && typeof entry === 'object') {
        return Object.entries(entry as Record<string, unknown>)
          .filter(([, target]) => typeof target === 'string')
          .map(([condition, target]) => `${key}:${condition}:${target}`);
      }
      return [];
    })
    .slice(0, 12);
  return entries.length > 0 ? entries : undefined;
}

function mapBin(value: unknown, packageName?: string): string[] | undefined {
  if (typeof value === 'string') {
    const cmd = packageName?.replace(/^@[^/]+\//, '') ?? '';
    return [cmd ? `${cmd} → ${value}` : value];
  }
  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, path]) => typeof path === 'string')
      .map(([cmd, path]) => `${cmd} → ${path}`)
      .slice(0, 8);
    return entries.length > 0 ? entries : undefined;
  }
  return undefined;
}

function inferPackageType(
  data: NpmViewResult
): NpmPackageResult['packageType'] {
  if (data.type === 'module' || data.module) return 'module';
  if (data.main) return 'commonjs';
  if (data.types || data.typings) return 'types-only';
  return 'unknown';
}

export function mapToResult(
  data: NpmViewResult,
  includeExtendedMetadata: boolean = false,
  source: 'cli' | 'registry' | 'cdn' = 'cli'
): NpmPackageResult {
  let repoUrl: string | null = null;
  let repositoryDirectory: string | undefined;
  if (data.repository) {
    if (typeof data.repository === 'string') {
      repoUrl = cleanRepoUrl(data.repository);
    } else {
      if (data.repository.url) {
        repoUrl = cleanRepoUrl(data.repository.url);
      }
      if (data.repository.directory) {
        repositoryDirectory = data.repository.directory.replace(/^\.\//, '');
      }
    }
  }

  let lastPublished: string | undefined;
  if (data.time) {
    const versionTime = data.version ? data.time[data.version] : undefined;
    const timeStr = versionTime || data.time.modified;
    if (timeStr) {
      lastPublished = timeStr;
    }
  }

  const result: NpmPackageResult = {
    name: data.name,
    npmUrl: `https://www.npmjs.com/package/${encodeURIComponent(data.name)}`,
    repoUrl,
    version: data.version || 'latest',
    mainEntry: data.main || null,
    moduleEntry: data.module || null,
    typeDefinitions: data.types || data.typings || null,
    packageType: inferPackageType(data),
    ...(repositoryDirectory ? { repositoryDirectory } : {}),
    ...(mapExports(data.exports) ? { exports: mapExports(data.exports) } : {}),
    ...(mapBin(data.bin, data.name)
      ? { bin: mapBin(data.bin, data.name) }
      : {}),
    lastPublished,
    source,
  };

  if (data.description) {
    result.description = data.description;
  }
  if (data.license) {
    result.license =
      typeof data.license === 'string' ? data.license : data.license.type;
  }

  if (includeExtendedMetadata) {
    if (data.author) {
      if (typeof data.author === 'string') {
        result.author = data.author;
      } else if (data.author.name) {
        result.author = data.author.name;
      }
    }

    if (data.keywords && data.keywords.length > 0) {
      result.keywords = data.keywords;
    }
    if (data.homepage) {
      result.homepage = data.homepage;
    }
    if (data.engines && Object.keys(data.engines).length > 0) {
      result.engines = data.engines;
    }
    if (data.dependencies && Object.keys(data.dependencies).length > 0) {
      result.dependencies = data.dependencies;
    }
    if (
      data.peerDependencies &&
      Object.keys(data.peerDependencies).length > 0
    ) {
      result.peerDependencies = data.peerDependencies;
    }
  }

  return result;
}

function getSearchItemRepoUrl(item: NpmCliSearchItem): string | null {
  if (typeof item.links?.repository === 'string') {
    return cleanRepoUrl(item.links.repository);
  }
  if (typeof item.repository === 'string') {
    return cleanRepoUrl(item.repository);
  }
  if (typeof item.repository?.url === 'string') {
    return cleanRepoUrl(item.repository.url);
  }
  return null;
}

export function mapSearchItemToResult(
  item: NpmCliSearchItem
): NpmPackageResult | null {
  if (!item.name) return null;
  const npmUrl =
    (item.links?.npm ?? '') ||
    `https://www.npmjs.com/package/${encodeURIComponent(item.name)}`;
  const homepage = item.links?.homepage ?? undefined;
  const result: NpmPackageResult = {
    name: item.name,
    npmUrl,
    repoUrl: getSearchItemRepoUrl(item),
    version: item.version ?? 'unknown',
    source: 'cli',
    ...(item.description ? { description: item.description } : {}),
    ...(homepage ? { homepage } : {}),
    ...(item.keywords && item.keywords.length > 0
      ? { keywords: item.keywords }
      : {}),
  };
  return result;
}

export function encodeRegistryPackageName(packageName: string): string {
  if (packageName.startsWith('@')) {
    return '@' + packageName.slice(1).replace('/', '%2F');
  }
  return packageName;
}

export function parseRegistrySearchTotal(
  total: string | number | undefined,
  fallback: number
): number {
  if (typeof total === 'number' && Number.isFinite(total)) return total;
  if (typeof total === 'string') {
    const parsed = Number.parseInt(total, 10);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}
