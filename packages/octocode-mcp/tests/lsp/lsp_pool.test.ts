/**
 * T3.2 — LSP client pool.
 *
 * Goal: replace spawn-per-request with a keyed pool so that
 *  - repeated requests against the same (workspaceRoot, languageId) re-use one warm client
 *  - idle clients are torn down after a configurable timeout
 *  - the pool can be explicitly cleared (foundation for the upcoming
 *    `lspRestart` tool)
 *
 * Mocks at the boundary: a stub factory replaces real LSP spawning.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { LspClientPool, type PoolKey } from '../../src/lsp/lspClientPool.js';

interface FakeClient {
  id: number;
  stopped: boolean;
  stop: () => Promise<void>;
}

let nextId = 0;
function makeFakeClient(): FakeClient {
  const self: FakeClient = {
    id: nextId++,
    stopped: false,
    stop: async () => {
      self.stopped = true;
    },
  };
  return self;
}

/**
 * Tiny narrowing helper so tests can `await acquireNonNull(pool, key)`
 * instead of writing `(await pool.acquire(key))!` or sprinkling
 * `as FakeClient` casts. Throws a descriptive error if the factory
 * unexpectedly returned null, which is more useful than a silent `!`.
 */
async function acquireNonNull(
  pool: LspClientPool<FakeClient>,
  key: PoolKey
): Promise<FakeClient> {
  const client = await pool.acquire(key);
  if (!client) {
    throw new Error(
      `Test setup: acquire(${key.workspaceRoot}, ${key.languageId}) returned null`
    );
  }
  return client;
}

describe('T3.2 — LspClientPool', () => {
  beforeEach(() => {
    nextId = 0;
  });

  it('returns the SAME client for the same key on repeated acquire()', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });

    const key: PoolKey = { workspaceRoot: '/repo', languageId: 'typescript' };
    const a = await pool.acquire(key);
    const b = await pool.acquire(key);

    expect(a).toBe(b);
    expect(factory).toHaveBeenCalledTimes(1);
  });

  it('returns DIFFERENT clients for different keys', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });

    const a = await pool.acquire({
      workspaceRoot: '/repo',
      languageId: 'typescript',
    });
    const b = await pool.acquire({
      workspaceRoot: '/repo',
      languageId: 'python',
    });
    const c = await pool.acquire({
      workspaceRoot: '/other',
      languageId: 'typescript',
    });

    expect(a).not.toBe(b);
    expect(a).not.toBe(c);
    expect(b).not.toBe(c);
    expect(factory).toHaveBeenCalledTimes(3);
  });

  it('returns null when the factory returns null (no language server available)', async () => {
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory: async () => null,
    });
    const client = await pool.acquire({
      workspaceRoot: '/repo',
      languageId: 'typescript',
    });
    expect(client).toBeNull();
  });

  it('deduplicates concurrent acquires for the same key (only one spawn)', async () => {
    let pendingResolve: ((c: FakeClient) => void) | undefined;
    const factory = vi.fn(
      () =>
        new Promise<FakeClient>(resolve => {
          pendingResolve = resolve;
        })
    );
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });

    const key: PoolKey = { workspaceRoot: '/repo', languageId: 'typescript' };
    const pA = pool.acquire(key);
    const pB = pool.acquire(key);
    pendingResolve!(makeFakeClient());

    const [a, b] = await Promise.all([pA, pB]);
    expect(a).toBe(b);
    expect(factory).toHaveBeenCalledTimes(1);
  });

  it('clearAll() stops every pooled client', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });

    const a = await acquireNonNull(pool, {
      workspaceRoot: '/r1',
      languageId: 'typescript',
    });
    const b = await acquireNonNull(pool, {
      workspaceRoot: '/r2',
      languageId: 'typescript',
    });

    await pool.clearAll();
    expect(a.stopped).toBe(true);
    expect(b.stopped).toBe(true);
    expect(pool.size()).toBe(0);
  });

  it('clear(key) stops only the matching client', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });
    const key1: PoolKey = { workspaceRoot: '/r1', languageId: 'typescript' };
    const key2: PoolKey = { workspaceRoot: '/r2', languageId: 'typescript' };
    const a = await acquireNonNull(pool, key1);
    const b = await acquireNonNull(pool, key2);

    await pool.clear(key1);
    expect(a.stopped).toBe(true);
    expect(b.stopped).toBe(false);
    expect(pool.size()).toBe(1);
  });

  it('evicts idle clients after the configured timeout', async () => {
    vi.useFakeTimers();
    try {
      const factory = vi.fn(async () => makeFakeClient());
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 25,
        factory,
      });

      const a = await acquireNonNull(pool, {
        workspaceRoot: '/r1',
        languageId: 'typescript',
      });

      // Advance past the idle timer and flush microtasks.
      await vi.advanceTimersByTimeAsync(30);
      expect(a.stopped).toBe(true);
      expect(pool.size()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('keys() returns active pool keys for diagnostics', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });
    const key1: PoolKey = { workspaceRoot: '/r1', languageId: 'typescript' };
    const key2: PoolKey = { workspaceRoot: '/r2', languageId: 'python' };

    await acquireNonNull(pool, key1);
    await acquireNonNull(pool, key2);

    expect(pool.keys()).toEqual(expect.arrayContaining([key1, key2]));
  });

  it('clear() is a no-op when called with a key that was never acquired', async () => {
    const factory = vi.fn(async () => makeFakeClient());
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 60_000,
      factory,
    });

    await pool.clear({ workspaceRoot: '/never', languageId: 'typescript' });

    expect(factory).not.toHaveBeenCalled();
    expect(pool.size()).toBe(0);
  });

  it('survives clear() racing against the idle-eviction timer for the same key', async () => {
    vi.useFakeTimers();
    try {
      const factory = vi.fn(async () => makeFakeClient());
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 20,
        factory,
      });
      const key: PoolKey = { workspaceRoot: '/r1', languageId: 'typescript' };

      const a = await acquireNonNull(pool, key);

      // Clear synchronously cancels the idle timer; ensure double-eviction
      // (idle + explicit) doesn't crash and the client is stopped exactly once.
      await pool.clear(key);
      expect(a.stopped).toBe(true);
      expect(pool.size()).toBe(0);

      // Now advance past the original idle window: the entry is already gone,
      // so the timer firing must hit the `if (!entry) return` guard cleanly.
      await vi.advanceTimersByTimeAsync(60);
      expect(pool.size()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('resets the idle timer on every acquire so warm clients are not evicted mid-use', async () => {
    vi.useFakeTimers();
    try {
      const factory = vi.fn(async () => makeFakeClient());
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 40,
        factory,
      });
      const key: PoolKey = { workspaceRoot: '/r1', languageId: 'typescript' };

      const a = await acquireNonNull(pool, key);
      // Re-acquire just before the original idle window expires — should
      // reset the timer rather than spawn a new client.
      await vi.advanceTimersByTimeAsync(25);
      const b = await acquireNonNull(pool, key);
      expect(a).toBe(b);
      expect(factory).toHaveBeenCalledTimes(1);

      // After a full idle window from the LAST acquire the client evicts.
      await vi.advanceTimersByTimeAsync(45);
      expect(a.stopped).toBe(true);
      expect(pool.size()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
