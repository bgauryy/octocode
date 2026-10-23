import type { DatabaseSync } from 'node:sqlite';
import { dbStamp, type AgentDb } from '../agentdb/db.js';
import { num, type Row } from '../agentdb/rows.js';
import { sameProcess } from '../shared/process.js';
import { HOUR_MS, isRecord, optionalString } from '../shared/util.js';
import { EditLog } from './edits.js';
import { toLease, toMember, type Lease, type Member } from './model.js';

/** An agent that has not refreshed its row for this long is hidden from `list` (a stalled process: suspended, or on a machine that slept). Only its process ending, or it leaving, drops the row and its mail. */
const STALE_MS = 60_000;
/** Leases live this long past the owner's last heartbeat, so a crashed owner frees its files quickly. */
export const LEASE_MS = 90_000;
const MESSAGE_RETENTION_MS = 24 * HOUR_MS;
const PRUNE_INTERVAL_MS = 60_000;

/** A member has not heartbeat for `STALE_MS`: its process may only be stalled, so it is hidden but keeps its mail and row. */
const stale = (seenAt: number, now: number): boolean => now - seenAt > STALE_MS;

/** The process a member row names: the pid and, where known, its start time, so a reused pid is not taken for it. */
interface ProcessIdentity {
  pid: number;
  started?: string | undefined;
}

const sameIdentity = (a: ProcessIdentity, b: ProcessIdentity): boolean => a.pid === b.pid && (a.started ?? null) === (b.started ?? null);

/** SQLite's "database is locked" (`SQLITE_BUSY`, extended codes included). */
const isBusy = (error: unknown): boolean => isRecord(error) && (Number(error['errcode']) & 0xff) === 5;

/** Busy wait for a background write (heartbeat, delivery): a locked database is retried on the next tick instead. */
const QUICK_BUSY_MS = 100;
/** Busy wait for a write an agent asked for (a lock, a message). */
const BUSY_MS = 5_000;

/**
 * The connection to the team tables and the `agents` roster: who joined, their heartbeats, and the one place a member
 * is retired. `TeamStore` (`store.ts`) adds the messages and the leases on top.
 */
export abstract class TeamRoster {
  protected readonly db: DatabaseSync;
  /** Who changed which file recently, in the same database. */
  readonly edits: EditLog;
  private prunedAt = -Infinity;
  /** Nested `quick` calls: only the outermost restores the long busy wait. */
  private quickDepth = 0;

  protected constructor(
    private readonly agent: AgentDb,
    readonly workspace: string,
    /** Whether `close` closes the connection: true for `open`, false for the process connection `shared` borrows. */
    private readonly ownsConnection: boolean,
  ) {
    this.db = agent.db;
    this.edits = new EditLog(agent.db, workspace);
  }

  /** Closes a connection `open` made; a `shared` store leaves the process connection open. */
  close(): void {
    if (this.ownsConnection) this.agent.close();
  }

  protected transaction<T>(work: () => T): T {
    return this.agent.transaction(work);
  }

  /** Moves whenever any process writes the database file (`dbStamp`): equal stamps mean a read would answer as it did. */
  stamp(): string {
    return dbStamp(this.agent.file);
  }

  /** Runs background work with a short busy wait, so a database held by another process never freezes this one. */
  quick<T>(work: () => T): T {
    if (this.quickDepth++ === 0) this.db.exec(`PRAGMA busy_timeout=${QUICK_BUSY_MS}`);
    try {
      return work();
    } finally {
      if (--this.quickDepth === 0) this.db.exec(`PRAGMA busy_timeout=${BUSY_MS}`);
    }
  }

  /**
   * The prune when a store opens: housekeeping, so it never makes the open fail or wait. While another process holds
   * the write lock (many Pi processes starting at once) it is skipped, and the next heartbeat's prune does it.
   */
  protected housekeep(): void {
    try {
      this.quick(() => this.prune());
    } catch (error) {
      if (!isBusy(error)) throw error;
    }
  }

  protected prune(now = Date.now()): void {
    if (now - this.prunedAt < PRUNE_INTERVAL_MS) return;
    this.db.prepare(`DELETE FROM messages WHERE created_at < ? AND NOT EXISTS (
      SELECT 1 FROM deliveries d WHERE d.message = messages.id AND d.completed_at IS NULL
        AND (d.dead_at IS NULL OR d.notified_at IS NULL)
    )`).run(now - MESSAGE_RETENTION_MS);
    this.db.prepare('DELETE FROM leases WHERE expires_at < ?').run(now);
    this.edits.prune(now);
    this.sweep(now);
    this.prunedAt = now;
  }

  /**
   * Heartbeat: refreshes the member row and runs the periodic prune (at most once a minute). The row is matched by id,
   * pid and start time, so a process never writes over a row another process holds; a row that is gone (swept while
   * this process was stalled) is inserted again. False when another process holds the id.
   */
  save(member: Member): boolean {
    this.prune(member.updatedAt);
    return this.refresh(member) || this.insert(member);
  }

  /** The heartbeat's `UPDATE`: matches the row only while it still names this process. */
  private refresh(member: Member): boolean {
    const { changes } = this.db
      .prepare(
        `UPDATE agents SET status = ?, task = ?, activity = ?, model = ?, seen_at = ?, tool_calls = ?, input = ?, cache_read = ?, output = ?, cost = ?, unpriced = ?
         WHERE id = ? AND pid = ? AND started IS ?`,
      )
      .run(
        member.status, member.goal ?? null, member.activity ?? null, member.model ?? null, member.updatedAt, member.toolCalls, member.input, member.cacheRead, member.output, member.cost,
        member.unpriced ? 1 : 0, member.id, member.pid, member.started ?? null,
      );
    return changes > 0;
  }

  /** Adds the row unless the id is taken (`ON CONFLICT DO NOTHING`, so a name held by another process is never overwritten); whether it was added. */
  private insert(member: Member): boolean {
    const row = this.db
      .prepare(
        `INSERT INTO agents (id, workspace, role, parent_id, pid, started, status, task, activity, model, joined_at, seen_at, tool_calls, input, cache_read, output, cost, unpriced)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO NOTHING RETURNING id`,
      )
      .get(member.id, this.workspace, member.role, member.parentId ?? null, member.pid, member.started ?? null, member.status, member.goal ?? null, member.activity ?? null, member.model ?? null,
        member.joinedAt, member.updatedAt, member.toolCalls, member.input, member.cacheRead, member.output, member.cost, member.unpriced ? 1 : 0);
    return row !== undefined;
  }

  /**
   * Takes the name `member.id` in one write transaction. False when another running process holds it: the caller
   * draws another name. A row of this same process (a second join) is refreshed; one whose process is gone is
   * retired first (its unread mail becomes dead letters) and the name is taken over.
   */
  join(member: Member): boolean {
    return this.transaction(() => {
      if (this.insert(member) || this.refresh(member)) return true;
      const holder = this.processOf(member.id);
      if (holder && sameProcess(holder.pid, holder.started)) return false;
      this.retire(member.id, Date.now());
      return this.insert(member);
    });
  }

  /**
   * Leave: the agent and its reservations go; the messages it never read become dead letters the sender can see. With
   * `owner` (a process leaving its own row) nothing is removed when the row now names another process.
   */
  remove(id: string, now = Date.now(), owner?: ProcessIdentity): boolean {
    return this.transaction(() => {
      if (owner) {
        const row = this.processOf(id);
        if (row && !sameIdentity(row, owner)) return false;
      }
      this.retire(id, now);
      return true;
    });
  }

  /** The process the row of `id` names; undefined when it has no row. */
  protected processOf(id: string): ProcessIdentity | undefined {
    const row = this.db.prepare('SELECT pid, started FROM agents WHERE id = ?').get(id) as Row | undefined;
    return row ? { pid: num(row['pid']), started: optionalString(row['started']) } : undefined;
  }

  /** Deletes the row, releases the leases and dead-letters the unread mail of `id`; the one place a member is dropped. */
  private retire(id: string, now: number): void {
    this.db.prepare('DELETE FROM agents WHERE id = ?').run(id);
    this.db.prepare('DELETE FROM leases WHERE owner = ?').run(id);
    this.db.prepare('UPDATE deliveries SET dead_at = ? WHERE recipient = ? AND delivered_at IS NULL AND dead_at IS NULL').run(now, id);
  }

  /**
   * Retires every member whose process is gone (`sameProcess`: dead, or a later process that got its pid): the one place
   * a member that did not leave is dropped and its unread mail dead-lettered. A stale member (alive, silent) is kept.
   * Each is judged outside the write lock and retired only if its row still names the process judged, so a member that
   * joined again under the same name meanwhile is left alone. What was retired is read back with `names`.
   */
  sweep(now = Date.now()): void {
    const rows = this.db.prepare('SELECT id, pid, started FROM agents').all() as Row[];
    for (const row of rows) {
      if (!sameProcess(num(row['pid']), optionalString(row['started']), now)) this.reap(String(row['id']), { pid: num(row['pid']), started: optionalString(row['started']) }, now);
    }
  }

  /** Retires `id` if its row still names `judged` (or is already gone, leaving leases of a member with no row to clean). */
  protected reap(id: string, judged: ProcessIdentity | undefined, now: number): boolean {
    return this.transaction(() => {
      const row = this.processOf(id);
      if (row && (judged === undefined || !sameIdentity(row, judged))) return false;
      this.retire(id, now);
      return true;
    });
  }

  /**
   * The machine slept from `since` to `now`: no agent could heartbeat or renew. Members still live at `since` (by
   * heartbeat) and leases not yet expired then get a fresh start from `now`, so the first process awake does not prune
   * its sleeping peers, drop their reservations or dead-letter their messages. Idempotent: every process that wakes
   * calls it. A member that stays silent after the wake goes `STALE_MS` later, its leases `LEASE_MS` later.
   */
  resume(since: number, now = Date.now()): void {
    this.transaction(() => {
      this.db.prepare('UPDATE agents SET seen_at = MAX(seen_at, ?) WHERE workspace = ? AND seen_at >= ?').run(now, this.workspace, since - STALE_MS);
      this.db.prepare('UPDATE leases SET expires_at = MAX(expires_at, ?) WHERE workspace = ? AND expires_at > ?').run(now + LEASE_MS, this.workspace, since);
    });
  }

  /** Every agent name in the shared database, whatever its workspace (ids are unique across all of them), live or not yet dropped. */
  names(): string[] {
    return (this.db.prepare('SELECT id FROM agents').all() as Row[]).map((row) => String(row['id']));
  }

  /**
   * Visible agents of this workspace, oldest first, with unanswered-message counts and held paths. A stale one (silent
   * for `STALE_MS`) or one whose process is gone is left out; nothing is removed here, `sweep` and `remove` do that.
   */
  list(now = Date.now()): Member[] {
    const rows = this.db
      .prepare(
        `SELECT a.*, (SELECT count(*) FROM deliveries d JOIN messages m ON m.id = d.message
                      WHERE d.recipient = a.id AND d.delivered_at IS NOT NULL AND d.completed_at IS NULL AND m.reply_required = 1) AS pending
         FROM agents a WHERE a.workspace = ? ORDER BY a.joined_at, a.id`,
      )
      .all(this.workspace) as Row[];
    const live = rows.map(toMember).filter((member) => !stale(member.updatedAt, now) && sameProcess(member.pid, member.started, now));
    const held = this.leases(now);
    return live.map((member) => {
      const locks = held.filter((lease) => lease.owner === member.id).map((lease) => (lease.kind === 'tree' ? `${lease.path}/` : lease.path));
      return locks.length > 0 ? { ...member, locks } : member;
    });
  }

  /** Ids of this workspace's agents that can still read mail: every one whose process runs, stale (stalled) ones included. */
  protected reachable(): Set<string> {
    const rows = this.db.prepare('SELECT id, pid, started FROM agents WHERE workspace = ?').all(this.workspace) as Row[];
    const now = Date.now();
    return new Set(rows.filter((row) => sameProcess(num(row['pid']), optionalString(row['started']), now)).map((row) => String(row['id'])));
  }

  leases(now = Date.now()): Lease[] {
    return (this.db.prepare('SELECT * FROM leases WHERE workspace = ? AND expires_at > ? ORDER BY id').all(this.workspace, now) as Row[]).map(toLease);
  }
}
