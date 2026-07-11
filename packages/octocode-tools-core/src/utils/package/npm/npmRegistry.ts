import { executeNpmCommand } from '../../exec/npm.js';

export const DEFAULT_NPM_REGISTRY = 'https://registry.npmjs.org';

let _cachedRegistryUrl: string | null = null;

export async function getNpmRegistryUrl(): Promise<string> {
  if (_cachedRegistryUrl) return _cachedRegistryUrl;

  try {
    const result = await executeNpmCommand(
      'config',
      ['get', 'registry', '--no-workspaces'],
      { timeout: 10000 }
    );
    if (!result.error && result.exitCode === 0) {
      const url = result.stdout.trim().replace(/\/+$/, '');
      if (url && url.startsWith('http')) {
        _cachedRegistryUrl = url;
        return url;
      }
    }
  } catch {
    void 0;
  }

  _cachedRegistryUrl = DEFAULT_NPM_REGISTRY;
  return DEFAULT_NPM_REGISTRY;
}

export function _resetNpmRegistryUrlCache(): void {
  _cachedRegistryUrl = null;
}

export async function checkNpmRegistryReachable(): Promise<boolean> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const f = globalThis.fetch;
    if (!f) return false;
    const res = await f(registryUrl, {
      method: 'HEAD',
      signal: AbortSignal.timeout(5000),
    });
    return res.ok;
  } catch {
    return false;
  }
}

export interface NpmViewResult {
  name: string;
  version: string;
  repository?: string | { url?: string; type?: string; directory?: string };
  main?: string;
  module?: string;
  type?: string;
  exports?: unknown;
  bin?: unknown;
  types?: string;
  typings?: string;
  description?: string;
  keywords?: string[];
  license?: string | { type?: string };
  homepage?: string;
  author?: string | { name?: string; email?: string; url?: string };
  maintainers?: Array<{ name?: string; email?: string }>;
  engines?: Record<string, string>;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  time?: {
    modified?: string;
    created?: string;
    [version: string]: string | undefined;
  };
}

export interface NpmRegistrySearchItem {
  name: string | null | undefined;
  version: string | null | undefined;
  description?: string | null;
  links?: {
    npm?: string | null;
    homepage?: string | null;
    repository?: string | null;
  };
}

export interface NpmCliSearchItem {
  name?: string;
  version?: string;
  description?: string;
  keywords?: string[];
  date?: string;
  links?: {
    npm?: string;
    homepage?: string;
    repository?: string;
    bugs?: string;
  };
  repository?: string | { url?: string; type?: string };
  score?: {
    final?: number;
    detail?: { quality?: number; popularity?: number; maintenance?: number };
  };
}
