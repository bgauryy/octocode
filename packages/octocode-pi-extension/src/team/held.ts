import { envInt } from '../shared/env.js';
import { covers } from './routing.js';
import type { Lease } from './model.js';

/** Minutes an idle session keeps renewing its reservations; past that they lapse so a forgotten lock frees itself. */
export const LEASE_IDLE_ENV = 'OCTOCODE_LEASE_IDLE_MINUTES';
const DEFAULT_LEASE_IDLE_MINUTES = 30;

/** How long an idle session keeps its reservations: `OCTOCODE_LEASE_IDLE_MINUTES` (0 = until the session ends), else 30 min. */
export function leaseIdleMs(env: NodeJS.ProcessEnv = process.env): number {
  const minutes = envInt(env, LEASE_IDLE_ENV, DEFAULT_LEASE_IDLE_MINUTES, { min: 0 });
  return minutes === 0 ? Number.POSITIVE_INFINITY : minutes * 60_000;
}

/**
 * The reservations this agent believes it holds (`kind:key` → path as locked), and the ones found lapsed since: a
 * lapsed reservation refuses edits until it is locked again, because a peer may have taken the path meanwhile.
 */
export class HeldLeases {
  private readonly held = new Map<string, string>();
  private readonly lapsed = new Map<string, string>();

  /** Whether any reservation is tracked, live or lapsed. */
  get any(): boolean {
    return this.held.size > 0 || this.lapsed.size > 0;
  }

  get holding(): boolean {
    return this.held.size > 0;
  }

  /** Every tracked key, live or lapsed. */
  keys(): string[] {
    return [...this.held.keys(), ...this.lapsed.keys()];
  }

  /** Record leases just granted, by their `kind:key`. */
  granted(leases: Array<{ key: string; lease: Lease }>): void {
    for (const { key, lease } of leases) {
      this.held.set(key, lease.path);
      this.lapsed.delete(key);
    }
  }

  release(keys: string[]): void {
    for (const key of keys) {
      this.held.delete(key);
      this.lapsed.delete(key);
    }
  }

  clear(): void {
    this.held.clear();
    this.lapsed.clear();
  }

  /**
   * The path of a reservation of ours covering `key` that is no longer live (`owned` lists the live ones, read only
   * when needed); it moves to lapsed. Undefined when none covers it or all that do are live.
   */
  lapsedCover(key: string, owned: () => Set<string>): string | undefined {
    for (const [lease, shown] of this.lapsed) if (covers(lease, key)) return shown;
    const mine = [...this.held].filter(([lease]) => covers(lease, key));
    if (mine.length === 0) return undefined;
    const live = owned();
    const gone = mine.find(([lease]) => !live.has(lease));
    if (!gone) return undefined;
    this.lapse(gone[0], gone[1]);
    return gone[1];
  }

  /** Moves every held reservation missing from `owned` to lapsed; returns their paths. */
  lost(owned: Set<string>): string[] {
    const lost = [...this.held].filter(([key]) => !owned.has(key));
    for (const [key, shown] of lost) this.lapse(key, shown);
    return lost.map(([, shown]) => shown);
  }

  private lapse(key: string, shown: string): void {
    this.held.delete(key);
    this.lapsed.set(key, shown);
  }
}
