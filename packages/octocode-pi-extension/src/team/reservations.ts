import { USER_SENDER } from '../shared/messages.js';
import { peerDbPath, peerRegistryDir } from '../shared/home.js';
import { visibleControls } from '../shared/sanitize.js';
import { HeldLeases } from './held.js';
import type { Lease, LeaseRequest } from './model.js';
import { PeerLeases, type PeerLease } from './peer-leases.js';
import { dbFailure, leaseAge, resolveLockPath } from './routing.js';
import { pathKey, type TeamStore } from './store.js';

/** What the reservations read from the team session they belong to; each is read when it is needed. */
interface ReservationsHost {
  /** This agent's id, once it joined. */
  id(): string | undefined;
  /** The team database only if someone already created it; throws when it exists but is unusable. */
  existing(): TeamStore | undefined;
  /** The session's working directory: lock paths resolve against it, as edits resolve. */
  cwd(): string | undefined;
  /** This Pi session's id: the agents-communication package skips its own session's leases. */
  sessionId(): string | undefined;
  /** Tells the agent something once (a lapsed reservation, an unreadable peer database); whether it was shown. */
  notice(text: string): boolean;
}

/**
 * This agent's file reservations: the team leases it holds (tracked, so a lapse is noticed) and the check every write
 * asks first, which also reads the agents-communication package's path leases.
 */
export class Reservations {
  private readonly held = new HeldLeases();
  /** The agents-communication package's path leases (read-only), opened on the first guarded write. */
  private peers: PeerLeases | undefined;

  constructor(private readonly host: ReservationsHost) {}

  /** Reserve paths (a trailing `/` marks a whole directory tree) for `owner`. */
  lock(db: TeamStore, owner: string, paths: string[], reason: string): { ok: true; leases: Lease[]; held: number } | { ok: false; conflicts: Array<{ path: string; heldBy: Lease }> } {
    // Stored resolved from this session's cwd, the way edits resolve; conflicts still show the paths as given.
    const cwd = this.host.cwd();
    const shown = new Map(paths.map((entry) => [resolveLockPath(cwd, entry), entry]));
    const requests: LeaseRequest[] = paths.map((entry) => ({ path: resolveLockPath(cwd, entry), kind: /[\\/]$/.test(entry) ? 'tree' : 'file', reason }));
    const result = db.lock(owner, requests);
    if (!result.ok) return { ok: false, conflicts: result.conflicts.map((conflict) => ({ ...conflict, path: shown.get(conflict.path) ?? conflict.path })) };
    this.held.granted(result.leases.map((lease) => ({ key: `${lease.kind}:${pathKey(db.workspace, lease.path)}`, lease })));
    // The store returns every lease this agent holds: keep the requested ones, and count the rest apart.
    const asked = new Set(requests.map((request) => `${request.kind}:${pathKey(db.workspace, request.path)}`));
    return { ok: true, leases: result.leases.filter((lease) => asked.has(`${lease.kind}:${pathKey(db.workspace, lease.path)}`)), held: result.leases.length };
  }

  unlock(db: TeamStore, owner: string, paths?: string[]): number {
    const resolved = paths?.map((entry) => resolveLockPath(this.host.cwd(), entry));
    const keys = resolved?.length ? resolved.flatMap((entry) => [`file:${pathKey(db.workspace, entry)}`, `tree:${pathKey(db.workspace, entry)}`]) : this.held.keys();
    this.held.release(keys);
    return db.unlock(owner, resolved);
  }

  /** Whether this agent takes part in the team: it joined, or still tracks reservations. */
  private collaborating(): boolean {
    return this.host.id() !== undefined || this.held.any;
  }

  /**
   * Whether this agent may change `file`: another agent's live lease, a reservation of ours that lapsed, or a
   * database error all forbid it. A database error fails closed only for an agent that collaborates; a solo session
   * (never joined, nothing held) keeps editing when the database is missing, foreign, too new or unusable.
   */
  private check(file: string): { lease?: Lease; peer?: PeerLease; lapsed?: string; error?: string } {
    try {
      const db = this.host.existing();
      if (db) {
        const lease = db.conflict(file, this.host.id() ?? USER_SENDER);
        if (lease) return { lease };
        const lapsed = this.lapsedCover(db, file);
        if (lapsed) return { lapsed };
      }
    } catch (error) {
      if (this.collaborating()) return { error: dbFailure(error) };
    }
    // Agents outside this team (Claude, Codex, Pi sessions that use agents-communication) reserve paths in their own database.
    const peer = this.peerLeases().conflict(file);
    return peer ? { peer } : {};
  }

  /** The package reads its home from the process environment, so this does too (not the team's injected `env`, which only carries a subagent's identity). */
  private peerLeases(): PeerLeases {
    return (this.peers ??= new PeerLeases(peerDbPath(), { registry: peerRegistryDir(), self: () => this.host.sessionId(), warn: (text) => this.host.notice(text) }));
  }

  /** A reservation of ours covering `file` that is no longer live in the database (now tracked as lapsed), if any. */
  private lapsedCover(db: TeamStore, file: string): string | undefined {
    const me = this.host.id();
    if (!me || !this.held.any) return undefined;
    return this.held.lapsedCover(pathKey(db.workspace, file), () => db.owned(me));
  }

  /** Why the file may not be changed (another agent's reservation, in this team or in agents-communication; our lapsed one; or an unreadable team database), or undefined. */
  reservation(absolute: string, display: string): string | undefined {
    const { lease, peer, lapsed, error } = this.check(absolute);
    if (error) return `Team database unavailable (${error}); retry the change.`;
    if (lapsed) return `Your reservation on ${lapsed} lapsed; lock it again before editing ${display}.`;
    if (peer) return visibleControls(`${display} is reserved by ${peer.name} (${peer.vendor} agent in agents-communication; ${peer.reason}; ${leaseAge(peer)}). Do other work, or message ${peer.name} through agents-communication and retry after they unlock.`);
    return lease ? visibleControls(`${display} is reserved by ${lease.owner} (${lease.reason}; ${leaseAge(lease)}). Do other work, or sendMessage ${lease.owner} and retry after they unlock.`) : undefined;
  }

  /** After a heartbeat: a held reservation the database no longer lists for `owner` lapsed (a peer may have taken it); says so once, `why` saying after what. */
  noticeLost(db: TeamStore, owner: string, now: number, why: string): void {
    if (!this.held.holding) return;
    // A heartbeat stall can let a lease lapse (and a peer take the path): say so once, and refuse edits until re-locked.
    const lost = this.held.lost(db.owned(owner, now));
    if (lost.length > 0) this.host.notice(`Your reservation on ${lost.join(', ')} lapsed${why}; lock it again before editing.`);
  }

  /** Forgets every reservation (the agent left). */
  clear(): void {
    this.held.clear();
  }

  /** Closes the peer database (session end). */
  close(): void {
    this.peers?.close();
    this.peers = undefined;
  }
}
