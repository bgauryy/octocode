import { createHash } from 'node:crypto';
import Config from '@npmcli/config';
import npmDefinitions from '@npmcli/config/lib/definitions/index.js';
import npmFetch from 'npm-registry-fetch';
import { resolveNpmConfigRoot } from '../../exec/npm.js';

export const DEFAULT_NPM_REGISTRY = 'https://registry.npmjs.org';

export interface NpmRegistryContext {
  registry: string;
  cacheIdentity: string;
  options: Record<string, unknown>;
}

export async function resolveNpmRegistryContext(
  packageName?: string,
  override?: string
): Promise<NpmRegistryContext> {
  // A new context observes login/logout, env changes and project configuration.
  // Give npm its own environment object: loading config must not mutate ours.
  const config = new Config({
    ...npmDefinitions,
    npmPath: resolveNpmConfigRoot(),
    argv: ['node', 'octocode', '--no-workspaces'],
    env: { ...process.env },
    cwd: process.cwd(),
  });
  await config.load();
  config.validate();
  const scope = packageName?.startsWith('@')
    ? packageName.split('/')[0]
    : undefined;
  const registry = String(
    override ??
      (scope ? config.get(`${scope}:registry`) : undefined) ??
      config.get('registry') ??
      DEFAULT_NPM_REGISTRY
  ).replace(/\/+$/, '');
  const url = new URL(registry);
  if (
    !['http:', 'https:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(
      'Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.'
    );
  }
  const options = { ...config.flat, registry };
  // Never store raw credentials in cache keys or output. The effective options
  // also separate scoped registries, user configuration and auth changes.
  const cacheIdentity = createHash('sha256')
    .update(
      JSON.stringify(
        Object.entries(options).sort(([a], [b]) => a.localeCompare(b))
      )
    )
    .digest('hex');
  return { registry, cacheIdentity, options };
}

export async function getNpmRegistryUrl(): Promise<string> {
  return (await resolveNpmRegistryContext()).registry;
}

export async function fetchNpmRegistryJson(
  context: NpmRegistryContext,
  path: string
): Promise<unknown> {
  try {
    return await npmFetch.json(`${context.registry}/${path}`, {
      ...context.options,
      registry: context.registry,
      cache: false,
      offline: false,
      preferOffline: false,
      retry: { retries: 1, minTimeout: 500, maxTimeout: 1000 },
      timeout: 8000,
    });
  } catch (error) {
    const status = (error as { statusCode?: number }).statusCode;
    // Raw provider causes may contain credentials or echoed response bodies.
    /* eslint-disable preserve-caught-error */
    if (status === 401 || status === 403) {
      throw new Error(
        `npm registry authentication failed (${status}). Check npm login and registry-scoped credentials.`
      );
    }
    if (status === 404) throw new Error('npm registry returned 404 Not Found.');
    // Provider response bodies can echo secrets; never pass them to tool output.
    throw new Error(
      status
        ? `npm registry request failed (HTTP ${status}).`
        : 'npm registry request failed: network or configuration error.'
    );
    /* eslint-enable preserve-caught-error */
  }
}

export async function checkNpmRegistryReachable(): Promise<boolean> {
  try {
    await fetchNpmRegistryJson(await resolveNpmRegistryContext(), '-/ping');
    return true;
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
  license?: string;
  date?: string;
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
