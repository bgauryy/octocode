import { resolve } from 'node:path';

export interface PoolKey {
  workspaceRoot: string;
  filePath: string;
  languageId: string;
  serverId?: string;
}

interface PooledClient {
  stop(): Promise<void>;
  /** Required health check: only confirmed live clients may be reused. */
  isAlive(): Promise<boolean>;
}

interface LspClientPoolOptions<T extends PooledClient> {
  idleTimeoutMs: number;

  factory: (key: PoolKey) => Promise<T | null>;
}

interface PoolEntry<T extends PooledClient> {
  client: T;
  timer: ReturnType<typeof setTimeout>;
  key: PoolKey;
}

export class LspClientPool<T extends PooledClient> {
  private readonly options: LspClientPoolOptions<T>;
  private readonly entries = new Map<string, PoolEntry<T>>();
  private readonly inflight = new Map<string, Promise<T | null>>();

  constructor(options: LspClientPoolOptions<T>) {
    this.options = options;
  }

  async acquire(key: PoolKey): Promise<T | null> {
    const k = serializeKey(key);
    const inflight = this.inflight.get(k);
    if (inflight) return inflight;

    // Register before running user code: even a synchronous factory throw must
    // clean up its own acquisition. Promise identity is the generation token;
    // clear removes it so neither startup nor health checks can revive clients.
    const promise = Promise.resolve().then(async () => {
      try {
        if (this.inflight.get(k) !== promise) return null;
        const cached = this.entries.get(k);
        if (cached) {
          const alive = await isEntryAlive(cached.client);
          if (this.inflight.get(k) !== promise) return null;
          // An idle timer can expire while the health check is pending.
          if (this.entries.get(k) === cached) {
            if (alive) {
              this.resetIdleTimer(k);
              return cached.client;
            }
            clearTimeout(cached.timer);
            this.entries.delete(k);
            void safeStop(cached.client);
          }
        }

        const client = await this.options.factory(key);
        if (!client) return null;
        if (this.inflight.get(k) !== promise) {
          await safeStop(client);
          return null;
        }
        const timer = this.startIdleTimer(k);
        this.entries.set(k, { client, timer, key });
        return client;
      } finally {
        if (this.inflight.get(k) === promise) this.inflight.delete(k);
      }
    });
    this.inflight.set(k, promise);
    return promise;
  }

  async clear(key: PoolKey): Promise<void> {
    const k = serializeKey(key);
    this.inflight.delete(k);
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    this.entries.delete(k);
    await safeStop(entry.client);
  }

  async clearAll(): Promise<void> {
    this.inflight.clear();
    const all = [...this.entries.values()];
    for (const entry of all) clearTimeout(entry.timer);
    this.entries.clear();
    await Promise.all(all.map(e => safeStop(e.client)));
  }

  size(): number {
    return this.entries.size;
  }

  keys(): PoolKey[] {
    return [...this.entries.values()].map(entry => entry.key);
  }

  has(key: PoolKey): boolean {
    const k = serializeKey(key);
    return this.entries.has(k) || this.inflight.has(k);
  }

  private resetIdleTimer(k: string): void {
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    entry.timer = this.startIdleTimer(k);
  }

  private startIdleTimer(k: string): ReturnType<typeof setTimeout> {
    const timer = setTimeout(() => {
      const entry = this.entries.get(k);
      if (!entry || entry.timer !== timer) return;
      this.entries.delete(k);
      void safeStop(entry.client);
    }, this.options.idleTimeoutMs);
    // This package is Node-only (napi-rs). The timer is always a NodeJS.Timeout
    // with an unref() method; calling it keeps the idle timer from preventing
    // a clean process exit when no real work is in flight.
    timer.unref();
    return timer;
  }
}

export function serializeKey(key: PoolKey): string {
  // Canonicalize the root: `/pkg` and `/pkg/` (or an unresolved relative path)
  // must map to the SAME pooled client, or equivalent roots silently spawn
  // parallel language servers with split index state.
  const root = resolve(key.workspaceRoot).replace(/(?<=.)[/\\]+$/, '');
  return `${key.serverId ?? key.languageId}\u0000${root}`;
}

async function isEntryAlive(client: PooledClient): Promise<boolean> {
  try {
    return await client.isAlive();
  } catch {
    // A failed check cannot establish that the connection is usable.
    return false;
  }
}

async function safeStop(client: PooledClient): Promise<void> {
  try {
    await client.stop();
  } catch {
    void 0;
  }
}
