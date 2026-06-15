import { describe, expect, it, vi } from 'vitest';
import { LspClientPool, type PoolKey } from '../src/lspClientPool.js';

type FakeClient = {
  readonly id: number;
  readonly stop: () => Promise<void>;
};

function key(
  workspaceRoot: string,
  languageId = 'typescript',
  serverId?: string
): PoolKey {
  return {
    workspaceRoot,
    filePath: `${workspaceRoot}/file.ts`,
    languageId,
    ...(serverId && { serverId }),
  };
}

function deferred<T>(): {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(r => {
    resolve = r;
  });
  return { promise, resolve };
}

describe('LspClientPool', () => {
  it('deduplicates inflight starts and reuses cached clients', async () => {
    const pending = deferred<FakeClient>();
    const client = { id: 1, stop: vi.fn().mockResolvedValue(undefined) };
    const factory = vi.fn(async () => pending.promise);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');

    const firstAcquire = pool.acquire(poolKey);
    const secondAcquire = pool.acquire(poolKey);
    pending.resolve(client);

    await expect(Promise.all([firstAcquire, secondAcquire])).resolves.toEqual([
      client,
      client,
    ]);
    expect(factory).toHaveBeenCalledTimes(1);
    expect(await pool.acquire(poolKey)).toBe(client);
    expect(pool.size()).toBe(1);
    expect(pool.keys()).toEqual([poolKey]);

    await pool.clearAll();
  });

  it('keeps null factories out of the pool', async () => {
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory: vi.fn().mockResolvedValue(null),
    });

    await expect(pool.acquire(key('/repo'))).resolves.toBeNull();
    expect(pool.size()).toBe(0);
  });

  it('stops clients on explicit clear and clearAll', async () => {
    const clientA = { id: 1, stop: vi.fn().mockResolvedValue(undefined) };
    const clientB = { id: 2, stop: vi.fn().mockRejectedValue(new Error('x')) };
    const clients = [clientA, clientB];
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory: vi.fn(async () => {
        const client = clients.shift();
        if (!client) throw new Error('missing client');
        return client;
      }),
    });
    const keyA = key('/repo-a', 'typescript', 'server-a');
    const keyB = key('/repo-b', 'typescript', 'server-b');

    await pool.acquire(keyA);
    await pool.acquire(keyB);
    await pool.clear(keyA);
    expect(clientA.stop).toHaveBeenCalledTimes(1);
    expect(pool.size()).toBe(1);

    await pool.clearAll();
    expect(clientB.stop).toHaveBeenCalledTimes(1);
    expect(pool.size()).toBe(0);
  });

  it('evicts idle clients', async () => {
    vi.useFakeTimers();
    try {
      const client = { id: 1, stop: vi.fn().mockResolvedValue(undefined) };
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 25,
        factory: vi.fn().mockResolvedValue(client),
      });

      await pool.acquire(key('/repo'));
      expect(pool.size()).toBe(1);
      await vi.advanceTimersByTimeAsync(25);
      expect(client.stop).toHaveBeenCalledTimes(1);
      expect(pool.size()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('ignores stale idle timers and missing reset entries', async () => {
    vi.useFakeTimers();
    try {
      const client = { id: 1, stop: vi.fn().mockResolvedValue(undefined) };
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 25,
        factory: vi.fn().mockResolvedValue(client),
      });
      const poolKey = key('/repo');
      await pool.acquire(poolKey);

      const internals = pool as unknown as {
        entries: Map<string, unknown>;
        resetIdleTimer(serializedKey: string): void;
      };
      const serializedKey = firstKey(internals.entries);
      internals.resetIdleTimer('missing');
      internals.entries.delete(serializedKey);

      await vi.advanceTimersByTimeAsync(25);
      expect(client.stop).not.toHaveBeenCalled();
      expect(pool.size()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
});

function firstKey(map: Map<string, unknown>): string {
  const key = map.keys().next().value;
  if (typeof key !== 'string') throw new Error('Expected a serialized key');
  return key;
}
