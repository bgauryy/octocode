import { describe, expect, it, vi } from 'vitest';
import { LspClientPool, type PoolKey } from '../../src/lsp/lspClientPool.js';

type FakeClient = {
  readonly id: number;
  readonly stop: () => Promise<void>;
  readonly isAlive: () => Promise<boolean>;
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
  it('invalidates startup on clear and stops its late client', async () => {
    const pending = deferred<FakeClient>();
    const client = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi.fn(() => pending.promise);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');
    const acquisition = pool.acquire(poolKey);
    await Promise.resolve();
    expect(factory).toHaveBeenCalledTimes(1);

    await pool.clear(poolKey);
    expect(pool.has(poolKey)).toBe(false);
    pending.resolve(client);

    await expect(acquisition).resolves.toBeNull();
    expect(client.stop).toHaveBeenCalledTimes(1);
    expect(pool.size()).toBe(0);
  });

  it('invalidates all pending startups without waiting for their factories', async () => {
    const pendingA = deferred<FakeClient>();
    const pendingB = deferred<FakeClient>();
    const clientA = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const clientB = {
      id: 2,
      stop: vi.fn().mockRejectedValue(new Error('stop')),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi
      .fn()
      .mockReturnValueOnce(pendingA.promise)
      .mockReturnValueOnce(pendingB.promise);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const keyA = key('/repo-a');
    const keyB = key('/repo-b');
    const acquisitions = [pool.acquire(keyA), pool.acquire(keyB)];
    await Promise.resolve();

    await pool.clearAll();
    expect(pool.has(keyA)).toBe(false);
    expect(pool.has(keyB)).toBe(false);
    pendingA.resolve(clientA);
    pendingB.resolve(clientB);

    await expect(Promise.all(acquisitions)).resolves.toEqual([null, null]);
    expect(clientA.stop).toHaveBeenCalledTimes(1);
    expect(clientB.stop).toHaveBeenCalledTimes(1);
    expect(pool.size()).toBe(0);
  });

  it('keeps a newer startup registered when an invalidated startup completes', async () => {
    const oldStart = deferred<FakeClient>();
    const newStart = deferred<FakeClient>();
    const oldClient = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const newClient = {
      id: 2,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi
      .fn()
      .mockReturnValueOnce(oldStart.promise)
      .mockReturnValueOnce(newStart.promise);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');
    const oldAcquire = pool.acquire(poolKey);
    await Promise.resolve();
    await pool.clear(poolKey);
    const newAcquire = pool.acquire(poolKey);
    await Promise.resolve();
    oldStart.resolve(oldClient);
    await expect(oldAcquire).resolves.toBeNull();
    expect(pool.has(poolKey)).toBe(true);

    const concurrentAcquire = pool.acquire(poolKey);
    newStart.resolve(newClient);
    await expect(Promise.all([newAcquire, concurrentAcquire])).resolves.toEqual(
      [newClient, newClient]
    );
    expect(factory).toHaveBeenCalledTimes(2);
    expect(oldClient.stop).toHaveBeenCalledTimes(1);
    expect(newClient.stop).not.toHaveBeenCalled();
    await pool.clearAll();
  });

  it.each([true, false])(
    'does not return or evict a replacement after a stale health check resolves %s',
    async alive => {
      const health = deferred<boolean>();
      const oldClient = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn(() => health.promise),
      };
      const newClient = {
        id: 2,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
      const factory = vi
        .fn()
        .mockResolvedValueOnce(oldClient)
        .mockResolvedValueOnce(newClient);
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 10_000,
        factory,
      });
      const poolKey = key('/repo');
      await pool.acquire(poolKey);
      const checking = pool.acquire(poolKey);
      await Promise.resolve();
      await pool.clear(poolKey);
      expect(await pool.acquire(poolKey)).toBe(newClient);
      health.resolve(alive);

      await expect(checking).resolves.toBeNull();
      expect(await pool.acquire(poolKey)).toBe(newClient);
      expect(factory).toHaveBeenCalledTimes(2);
      expect(oldClient.stop).toHaveBeenCalledTimes(1);
      expect(newClient.stop).not.toHaveBeenCalled();
      await pool.clearAll();
    }
  );

  it.each(['throw', 'reject', 'null'])(
    'allows retry after a factory %s',
    async failure => {
      const client = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
      const factory = vi.fn((): Promise<FakeClient | null> => {
        if (failure === 'throw') throw new Error('startup');
        if (failure === 'reject') return Promise.reject(new Error('startup'));
        return Promise.resolve(null);
      });
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 10_000,
        factory,
      });
      const poolKey = key('/repo');
      if (failure === 'null')
        await expect(pool.acquire(poolKey)).resolves.toBeNull();
      else await expect(pool.acquire(poolKey)).rejects.toThrow('startup');
      expect(pool.has(poolKey)).toBe(false);
      factory.mockResolvedValue(client);
      expect(await pool.acquire(poolKey)).toBe(client);
      expect(factory).toHaveBeenCalledTimes(2);
      await pool.clearAll();
    }
  );

  it('stops a late startup without replacing an already cached newer client', async () => {
    const pending = deferred<FakeClient>();
    const oldClient = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const newClient = {
      id: 2,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi
      .fn()
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(newClient);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');
    const oldAcquire = pool.acquire(poolKey);
    await Promise.resolve();
    await pool.clearAll();
    expect(await pool.acquire(poolKey)).toBe(newClient);
    pending.resolve(oldClient);

    await expect(oldAcquire).resolves.toBeNull();
    expect(await pool.acquire(poolKey)).toBe(newClient);
    expect(oldClient.stop).toHaveBeenCalledTimes(1);
    expect(newClient.stop).not.toHaveBeenCalled();
    await pool.clearAll();
  });

  it('deduplicates concurrent health checks and replacement starts', async () => {
    const health = deferred<boolean>();
    const oldClient = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn(() => health.promise),
    };
    const newClient = {
      id: 2,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi
      .fn()
      .mockResolvedValueOnce(oldClient)
      .mockResolvedValueOnce(newClient);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');
    await pool.acquire(poolKey);
    const first = pool.acquire(poolKey);
    const second = pool.acquire(poolKey);
    await Promise.resolve();
    expect(oldClient.isAlive).toHaveBeenCalledTimes(1);
    health.resolve(false);

    await expect(Promise.all([first, second])).resolves.toEqual([
      newClient,
      newClient,
    ]);
    expect(oldClient.stop).toHaveBeenCalledTimes(1);
    expect(factory).toHaveBeenCalledTimes(2);
    await pool.clearAll();
  });

  it('does not return a client evicted while its health check was pending', async () => {
    vi.useFakeTimers();
    try {
      const health = deferred<boolean>();
      const oldClient = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn(() => health.promise),
      };
      const newClient = {
        id: 2,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
      const factory = vi
        .fn()
        .mockResolvedValueOnce(oldClient)
        .mockResolvedValueOnce(newClient);
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 25,
        factory,
      });
      const poolKey = key('/repo');
      await pool.acquire(poolKey);
      const checking = pool.acquire(poolKey);
      await vi.advanceTimersByTimeAsync(25);
      expect(oldClient.stop).toHaveBeenCalledTimes(1);
      health.resolve(true);

      await expect(checking).resolves.toBe(newClient);
      expect(oldClient.stop).toHaveBeenCalledTimes(1);
      expect(await pool.acquire(poolKey)).toBe(newClient);
      await pool.clearAll();
    } finally {
      vi.useRealTimers();
    }
  });

  it('renews the idle timeout after a cached acquisition', async () => {
    vi.useFakeTimers();
    try {
      const client = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
      const pool = new LspClientPool<FakeClient>({
        idleTimeoutMs: 25,
        factory: vi.fn().mockResolvedValue(client),
      });
      const poolKey = key('/repo');
      await pool.acquire(poolKey);
      await vi.advanceTimersByTimeAsync(20);
      expect(await pool.acquire(poolKey)).toBe(client);
      await vi.advanceTimersByTimeAsync(20);
      expect(client.stop).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(5);
      expect(client.stop).toHaveBeenCalledTimes(1);
      expect(pool.has(poolKey)).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('deduplicates inflight starts and reuses cached clients', async () => {
    const pending = deferred<FakeClient>();
    const client = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
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
    const clientA = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const clientB = {
      id: 2,
      stop: vi.fn().mockRejectedValue(new Error('x')),
      isAlive: vi.fn().mockResolvedValue(true),
    };
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
      const client = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
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

  it('evicts a crashed client and creates a fresh one on next acquire, without waiting for the idle timer', async () => {
    const clientA = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const clientB = {
      id: 2,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const clients = [clientA, clientB];
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory: vi.fn(async () => {
        const client = clients.shift();
        if (!client) throw new Error('missing client');
        return client;
      }),
    });
    const poolKey = key('/repo');

    expect(await pool.acquire(poolKey)).toBe(clientA);
    expect(await pool.acquire(poolKey)).toBe(clientA); // still alive: cached, no factory call

    // Backing process crashes mid-session.
    clientA.isAlive.mockResolvedValue(false);

    expect(await pool.acquire(poolKey)).toBe(clientB);
    expect(clientA.stop).toHaveBeenCalledTimes(1); // evicted, not left for the idle timer
    expect(pool.size()).toBe(1);
    expect(pool.keys()).toEqual([poolKey]);
  });

  it('replaces a client whose required health check fails instead of asserting it is alive', async () => {
    const client = {
      id: 1,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockRejectedValue(new Error('connection unavailable')),
    };
    const replacement = {
      id: 2,
      stop: vi.fn().mockResolvedValue(undefined),
      isAlive: vi.fn().mockResolvedValue(true),
    };
    const factory = vi
      .fn()
      .mockResolvedValueOnce(client)
      .mockResolvedValueOnce(replacement);
    const pool = new LspClientPool<FakeClient>({
      idleTimeoutMs: 10_000,
      factory,
    });
    const poolKey = key('/repo');

    expect(await pool.acquire(poolKey)).toBe(client);
    expect(await pool.acquire(poolKey)).toBe(replacement);

    expect(factory).toHaveBeenCalledTimes(2);
    expect(client.stop).toHaveBeenCalledTimes(1);
    await pool.clearAll();
  });

  it('ignores stale idle timers and missing reset entries', async () => {
    vi.useFakeTimers();
    try {
      const client = {
        id: 1,
        stop: vi.fn().mockResolvedValue(undefined),
        isAlive: vi.fn().mockResolvedValue(true),
      };
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
