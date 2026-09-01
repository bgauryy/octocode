import { AsyncLocalStorage } from 'node:async_hooks';

interface CacheTraceRoot {
  hit: boolean;
}

interface CacheTraceContext {
  root: CacheTraceRoot;
  depth: number;
  role: CacheRole;
}

export type CacheRole = 'response' | 'helper';

const cacheTrace = new AsyncLocalStorage<CacheTraceContext>();

/** Trace cache activity for one independently processed bulk query. */
export async function traceResponseCache<T>(
  operation: () => Promise<T>
): Promise<{ value: T; cacheHit: boolean }> {
  const root: CacheTraceRoot = { hit: false };
  const value = await cacheTrace.run(
    { root, depth: 0, role: 'response' },
    operation
  );
  return { value, cacheHit: root.hit };
}

/** Preserve cache nesting so helper-cache hits do not label a fresh response. */
export function runCacheLayer<T>(
  operation: () => Promise<T>,
  role: CacheRole = 'response'
): Promise<T> {
  const parent = cacheTrace.getStore();
  if (!parent) return operation();
  return cacheTrace.run(
    { root: parent.root, depth: parent.depth + 1, role },
    operation
  );
}

/** Mark a direct hit or a query's outermost cache layer as its response source. */
export function markResponseCacheHit(): void {
  const context = cacheTrace.getStore();
  if (context && context.depth <= 1 && context.role === 'response') {
    context.root.hit = true;
  }
}
