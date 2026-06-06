export {
  resolveConfigSync,
  resolveConfig,
  getConfig,
  getConfigSync,
  reloadConfig,
  invalidateConfigCache,
  _resetConfigCache,
  _getCacheState,
} from './resolverCache.js';

import { getConfigSync } from './resolverCache.js';

export function getConfigValue<T = unknown>(path: string): T | undefined {
  const config = getConfigSync();
  const parts = path.split('.');

  let current: unknown = config;
  for (const part of parts) {
    if (current === null || current === undefined) return undefined;
    if (typeof current !== 'object') return undefined;
    current = (current as Record<string, unknown>)[part];
  }

  return current as T;
}
