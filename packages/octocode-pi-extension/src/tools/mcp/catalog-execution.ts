export interface McpCatalogIdentity {
  workspace: string;
  server: string;
  signature: string;
}

interface CatalogFlight<T> {
  identity: McpCatalogIdentity;
  promise: Promise<T>;
}

/** Own exact-schema freshness, duplicate suppression, and invalidation fencing. */
export class McpCatalogExecution<T> {
  private readonly fresh = new Map<string, McpCatalogIdentity>();
  private readonly pending = new Map<string, CatalogFlight<T>>();

  private key(identity: McpCatalogIdentity): string {
    return JSON.stringify([
      identity.workspace,
      identity.server,
      identity.signature,
    ]);
  }

  isFresh(identity: McpCatalogIdentity): boolean {
    return this.fresh.has(this.key(identity));
  }

  resolve(
    identity: McpCatalogIdentity,
    discover: () => Promise<T>,
    commit: (value: T) => void,
  ): Promise<T> {
    const key = this.key(identity);
    const existing = this.pending.get(key);
    if (existing) return existing.promise;

    const flight = { identity, promise: undefined as unknown as Promise<T> };
    flight.promise = Promise.resolve()
      .then(discover)
      .then((value) => {
        if (this.pending.get(key) !== flight) {
          throw new Error(
            `MCP catalog changed while discovering ${identity.server}; retry the call`,
          );
        }
        commit(value);
        this.fresh.set(key, identity);
        return value;
      })
      .finally(() => {
        if (this.pending.get(key) === flight) this.pending.delete(key);
      });
    this.pending.set(key, flight);
    return flight.promise;
  }

  invalidateWorkspace(workspace: string): void {
    this.invalidate((identity) => identity.workspace === workspace);
  }

  invalidateServer(server: string): void {
    this.invalidate((identity) => identity.server === server);
  }

  clear(): void {
    this.fresh.clear();
    this.pending.clear();
  }

  private invalidate(predicate: (identity: McpCatalogIdentity) => boolean): void {
    for (const [key, identity] of this.fresh) {
      if (predicate(identity)) this.fresh.delete(key);
    }
    for (const [key, flight] of this.pending) {
      if (predicate(flight.identity)) this.pending.delete(key);
    }
  }
}
