export interface PoolKey {
  workspaceRoot: string;
  filePath: string;
  languageId: string;
  serverId?: string;
}

interface PooledClient {
  stop(): Promise<void>;
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

    const cached = this.entries.get(k);
    if (cached) {
      this.resetIdleTimer(k);
      return cached.client;
    }

    const inflight = this.inflight.get(k);
    if (inflight) return inflight;

    const promise = (async () => {
      try {
        const client = await this.options.factory(key);
        if (!client) return null;
        const timer = this.startIdleTimer(k);
        this.entries.set(k, { client, timer, key });
        return client;
      } finally {
        this.inflight.delete(k);
      }
    })();
    this.inflight.set(k, promise);
    return promise;
  }

  async clear(key: PoolKey): Promise<void> {
    const k = serializeKey(key);
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    this.entries.delete(k);
    await safeStop(entry.client);
  }

  async clearAll(): Promise<void> {
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

  private resetIdleTimer(k: string): void {
    const entry = this.entries.get(k);
    if (!entry) return;
    clearTimeout(entry.timer);
    entry.timer = this.startIdleTimer(k);
  }

  private startIdleTimer(k: string): ReturnType<typeof setTimeout> {
    const timer = setTimeout(() => {
      const entry = this.entries.get(k);
      if (!entry) return;
      this.entries.delete(k);
      void safeStop(entry.client);
    }, this.options.idleTimeoutMs);
    if (typeof timer === 'object' && 'unref' in timer) {
      timer.unref();
    }
    return timer;
  }
}

function serializeKey(key: PoolKey): string {
  return `${key.serverId ?? key.languageId}\u0000${key.workspaceRoot}`;
}

async function safeStop(client: PooledClient): Promise<void> {
  try {
    await client.stop();
  } catch {
    void 0;
  }
}
