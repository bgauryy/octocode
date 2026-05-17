/**
 * Keyed LSP client pool (T3.2).
 *
 * Replaces the spawn-per-request model in `manager.ts` with a pool
 * keyed on `(workspaceRoot, languageId)`. Repeated requests against
 * the same project re-use the same warm client, so tsserver gets to
 * keep its in-memory project graph instead of re-indexing on every
 * call.
 *
 * Design notes:
 *  - Generic over the client type so we can unit-test against a stub.
 *  - Acquires for the same key are deduplicated mid-flight (no thundering
 *    herd on cold spawn).
 *  - Each acquire resets the per-entry idle timer; when it fires we stop()
 *    the client and remove it from the pool. New requests get a fresh
 *    client lazily.
 *  - `clear(key)` and `clearAll()` are the public restart primitives —
 *    they're what the upcoming `lspRestart` tool will call.
 *
 * @module lsp/lspClientPool
 */

export interface PoolKey {
  workspaceRoot: string;
  languageId: string;
}

export interface PooledClient {
  stop(): Promise<void>;
}

export interface LspClientPoolOptions<T extends PooledClient> {
  /** Idle timeout in ms before a pooled client is torn down. */
  idleTimeoutMs: number;
  /** Spawn a new client. Return `null` when no server is available. */
  factory: (key: PoolKey) => Promise<T | null>;
}

interface PoolEntry<T extends PooledClient> {
  client: T;
  timer: ReturnType<typeof setTimeout>;
}

export class LspClientPool<T extends PooledClient> {
  private readonly options: LspClientPoolOptions<T>;
  private readonly entries = new Map<string, PoolEntry<T>>();
  private readonly inflight = new Map<string, Promise<T | null>>();

  constructor(options: LspClientPoolOptions<T>) {
    this.options = options;
  }

  /**
   * Return a (possibly warm) client for the given key. Resolves to
   * `null` when the factory cannot produce one (e.g. no language
   * server installed for that file type).
   */
  async acquire(key: PoolKey): Promise<T | null> {
    const k = serializeKey(key);

    const cached = this.entries.get(k);
    if (cached) {
      this.resetIdleTimer(k);
      return cached.client;
    }

    // De-dupe concurrent acquires for the same key.
    const inflight = this.inflight.get(k);
    if (inflight) return inflight;

    const promise = (async () => {
      try {
        const client = await this.options.factory(key);
        if (!client) return null;
        const timer = this.startIdleTimer(k);
        this.entries.set(k, { client, timer });
        return client;
      } finally {
        this.inflight.delete(k);
      }
    })();
    this.inflight.set(k, promise);
    return promise;
  }

  /** Tear down the client for `key`, if present. */
  async clear(key: PoolKey): Promise<void> {
    const k = serializeKey(key);
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    this.entries.delete(k);
    await safeStop(entry.client);
  }

  /** Tear down every pooled client. Used by `lspRestart`. */
  async clearAll(): Promise<void> {
    const all = [...this.entries.values()];
    for (const entry of all) clearTimeout(entry.timer);
    this.entries.clear();
    await Promise.all(all.map(e => safeStop(e.client)));
  }

  /** Number of currently-pooled clients (for tests and metrics). */
  size(): number {
    return this.entries.size;
  }

  private resetIdleTimer(k: string): void {
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    entry.timer = this.startIdleTimer(k);
  }

  private startIdleTimer(k: string): ReturnType<typeof setTimeout> {
    return setTimeout(() => {
      const entry = this.entries.get(k);
      if (!entry) return;
      this.entries.delete(k);
      // Fire-and-forget — stop() errors are non-actionable here.
      void safeStop(entry.client);
    }, this.options.idleTimeoutMs);
  }
}

function serializeKey(key: PoolKey): string {
  return `${key.languageId}\u0000${key.workspaceRoot}`;
}

async function safeStop(client: PooledClient): Promise<void> {
  try {
    await client.stop();
  } catch {
    // Already shutting down — no useful recovery here.
  }
}
